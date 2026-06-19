use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::Path;
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
