use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TmuxError {
    #[error("invalid session name: {0:?}")]
    InvalidName(String),
    #[error("invalid pane id: {0:?}")]
    InvalidPane(String),
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("tmux exited non-zero: {0}")]
    Tmux(String),
}

/// A tmux pane as the daemon sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub session: String,
    pub pane_id: String,
    pub working_dir: String,
}

/// Validate a session name. Allowed: alnum, dot, dash, underscore, 1..=64 chars.
/// Spec section 4 trust boundary: no shell injection.
pub fn validate_session_name(name: &str) -> Result<(), TmuxError> {
    if name.is_empty() || name.len() > 64 {
        return Err(TmuxError::InvalidName(name.to_string()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(TmuxError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Validate a pane id. Must match `^%[0-9]+$`.
pub fn validate_pane_id(id: &str) -> Result<(), TmuxError> {
    if id.len() < 2 || !id.starts_with('%') {
        return Err(TmuxError::InvalidPane(id.to_string()));
    }
    if !id[1..].chars().all(|c| c.is_ascii_digit()) {
        return Err(TmuxError::InvalidPane(id.to_string()));
    }
    Ok(())
}

#[async_trait]
pub trait Tmux: Send + Sync {
    async fn new_session(&self, name: &str, working_dir: &str) -> Result<String, TmuxError>;
    async fn has_session(&self, name: &str) -> bool;
    async fn switch_client(&self, target: &str) -> Result<(), TmuxError>;
    async fn list_panes(&self) -> Result<Vec<Pane>, TmuxError>;
    async fn kill_session(&self, name: &str) -> Result<(), TmuxError>;
    async fn capture_pane(&self, pane: &str, lines: u16) -> Result<String, TmuxError>;
}

/// In-memory `Tmux` for unit tests. The real `RealTmux` (using
/// `tmux_interface` or subprocess) is wired in a later task; for now
/// `MockTmux` is what every test in the crate uses.
pub struct MockTmux {
    next_pane: Mutex<u32>,
    sessions: Mutex<HashMap<String, (String, String)>>, // name -> (pane_id, working_dir)
}

impl MockTmux {
    pub fn new() -> Self {
        Self {
            next_pane: Mutex::new(0),
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MockTmux {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tmux for MockTmux {
    async fn new_session(&self, name: &str, working_dir: &str) -> Result<String, TmuxError> {
        validate_session_name(name)?;
        let path = Path::new(working_dir);
        let _ = path; // not used in mock
        let mut n = self.next_pane.lock();
        *n += 1;
        let pane_id = format!("%{n}");
        self.sessions
            .lock()
            .insert(name.to_string(), (pane_id.clone(), working_dir.to_string()));
        Ok(pane_id)
    }

    async fn has_session(&self, name: &str) -> bool {
        self.sessions.lock().contains_key(name)
    }

    async fn switch_client(&self, target: &str) -> Result<(), TmuxError> {
        if self.sessions.lock().contains_key(target) {
            Ok(())
        } else {
            Err(TmuxError::NotFound(target.to_string()))
        }
    }

    async fn list_panes(&self) -> Result<Vec<Pane>, TmuxError> {
        Ok(self
            .sessions
            .lock()
            .iter()
            .map(|(name, (pane_id, wd))| Pane {
                session: name.clone(),
                pane_id: pane_id.clone(),
                working_dir: wd.clone(),
            })
            .collect())
    }

    async fn kill_session(&self, name: &str) -> Result<(), TmuxError> {
        self.sessions.lock().remove(name);
        Ok(())
    }

    async fn capture_pane(&self, _pane: &str, _lines: u16) -> Result<String, TmuxError> {
        Ok(String::new())
    }
}

/// Run `tmux -V` and return true if it exits 0 with a parseable version
/// >= 2.6 (the version that introduced `status-interval 1` reliably).
pub fn tmux_version_ok() -> bool {
    let out = std::process::Command::new("tmux").arg("-V").output();
    let Ok(out) = out else { return false };
    if !out.status.success() {
        return false;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    // Output: "tmux 3.4"
    let Some(rest) = s.strip_prefix("tmux ") else {
        return false;
    };
    let ver_str = rest.trim().split('.').next().unwrap_or("0");
    ver_str.parse::<u32>().is_ok_and(|v| v >= 2)
        && rest
            .split('.')
            .nth(1)
            .and_then(|x| x.parse::<u32>().ok())
            .unwrap_or(0)
            >= 6
}

/// `Tmux` implementation that shells out to the `tmux` binary via
/// `tokio::process::Command`. Used in production; tests use `MockTmux`.
pub struct RealTmux {
    tmux: PathBuf,
}

impl Default for RealTmux {
    fn default() -> Self {
        Self::new()
    }
}

impl RealTmux {
    pub fn new() -> Self {
        Self {
            tmux: PathBuf::from("tmux"),
        }
    }
    pub fn with_binary(p: PathBuf) -> Self {
        Self { tmux: p }
    }
}

#[async_trait]
impl Tmux for RealTmux {
    async fn new_session(&self, name: &str, working_dir: &str) -> Result<String, TmuxError> {
        validate_session_name(name)?;
        let out = tokio::process::Command::new(&self.tmux)
            .args(["new-session", "-d", "-s", name, "-c", working_dir])
            .output()
            .await
            .map_err(TmuxError::Io)?;
        if !out.status.success() {
            return Err(TmuxError::Tmux(
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ));
        }
        // The new pane id is %N; we can fetch it with list-panes.
        let panes = self.list_panes().await?;
        panes
            .into_iter()
            .find(|p| p.session == name)
            .map(|p| p.pane_id)
            .ok_or_else(|| TmuxError::Tmux(format!("no pane found for {name}")))
    }

    async fn has_session(&self, name: &str) -> bool {
        let Ok(out) = tokio::process::Command::new(&self.tmux)
            .args(["has-session", "-t", name])
            .output()
            .await
        else {
            return false;
        };
        out.status.success()
    }

    async fn switch_client(&self, target: &str) -> Result<(), TmuxError> {
        let out = tokio::process::Command::new(&self.tmux)
            .args(["switch-client", "-t", target])
            .output()
            .await
            .map_err(TmuxError::Io)?;
        if out.status.success() {
            Ok(())
        } else {
            Err(TmuxError::NotFound(target.to_string()))
        }
    }

    async fn list_panes(&self) -> Result<Vec<Pane>, TmuxError> {
        let out = tokio::process::Command::new(&self.tmux)
            .args([
                "list-panes",
                "-a",
                "-F",
                "#{session_name} #{pane_id} #{pane_current_path}",
            ])
            .output()
            .await
            .map_err(TmuxError::Io)?;
        if !out.status.success() {
            return Err(TmuxError::Tmux(
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ));
        }
        let s = String::from_utf8_lossy(&out.stdout);
        s.lines()
            .filter_map(|line| {
                let mut it = line.splitn(3, ' ');
                Some(Pane {
                    session: it.next()?.to_string(),
                    pane_id: it.next()?.to_string(),
                    working_dir: it.next()?.to_string(),
                })
            })
            .collect::<Vec<_>>()
            .pipe(Ok)
    }

    async fn kill_session(&self, name: &str) -> Result<(), TmuxError> {
        let out = tokio::process::Command::new(&self.tmux)
            .args(["kill-session", "-t", name])
            .output()
            .await
            .map_err(TmuxError::Io)?;
        if out.status.success() {
            Ok(())
        } else {
            // Already gone is fine; surface other errors.
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("can't find session") {
                Ok(())
            } else {
                Err(TmuxError::Tmux(stderr.into_owned()))
            }
        }
    }

    async fn capture_pane(&self, pane: &str, lines: u16) -> Result<String, TmuxError> {
        let out = tokio::process::Command::new(&self.tmux)
            .args(["capture-pane", "-p", "-t", pane, "-S", &format!("-{lines}")])
            .output()
            .await
            .map_err(TmuxError::Io)?;
        if !out.status.success() {
            return Err(TmuxError::Tmux(
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

// Tiny `pipe` helper used by `list_panes` to convert `Vec<T>` into
// `Result<Vec<T>, E>` without an extra intermediate binding.
trait Pipe: Sized {
    fn pipe<R>(self, f: impl FnOnce(Self) -> R) -> R {
        f(self)
    }
}
impl<T> Pipe for T {}
