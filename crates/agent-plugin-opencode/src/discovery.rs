//! Discover tmux panes whose foreground process is `opencode`.
//!
//! The plugin runs `tmux list-panes -a` to enumerate all panes on the host,
//! then checks each pane's foreground process (via `/proc/<pid>/comm` on
//! Linux or `ps -p <pid> -o comm=` on macOS) to determine whether it is
//! running the `opencode` binary. The resulting list is passed to
//! [`agentd_plugin_sdk::AgentdClient::discover`] so the daemon can register
//! each pane as a session.

use std::path::{Path, PathBuf};

/// A tmux pane that the plugin determined to be running `opencode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpencodePane {
    /// Name of the tmux session the pane belongs to.
    pub tmux_session: String,
    /// Stable tmux pane id (e.g. `%0`).
    pub pane_id: String,
    /// PID of the pane's foreground process.
    pub pane_pid: u32,
    /// Working directory reported by tmux for the pane.
    pub working_dir: PathBuf,
}

/// A raw pane row as emitted by `tmux list-panes -F`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawPane {
    /// Name of the tmux session the pane belongs to.
    pub session: String,
    /// Stable tmux pane id.
    pub pane_id: String,
    /// PID of the pane's foreground process.
    pub pid: u32,
    /// Working directory reported by tmux for the pane.
    pub working_dir: PathBuf,
    /// Name of the pane's foreground process as reported by tmux
    /// (`#{pane_current_command}`). May contain spaces (e.g.
    /// `"bash -c 'opencode run ...'"`); the parser preserves them
    /// as a single field.
    pub pane_current_command: String,
}

/// Error type for [`discover_opencode_panes`].
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The `tmux` binary could not be spawned (typically: not on `$PATH`).
    #[error("failed to run `tmux`: {0}")]
    TmuxSpawn(#[source] std::io::Error),
    /// `tmux` exited with a non-zero status.
    #[error("`tmux` exited {status}: {stderr}")]
    TmuxFailed {
        /// `tmux`'s exit status code.
        status: i32,
        /// Captured stderr from `tmux`.
        stderr: String,
    },
}

/// Scan all tmux panes on the host and return those running `opencode`.
///
/// The current strategy is best-effort and Linux-first:
/// 1. Shell out to `tmux list-panes -a -F '<session> <pane_id> <pid> <path> <comm>'`.
/// 2. For each pane, keep it iff its `pane_current_command` matches
///    `opencode` (case-insensitive). As a fallback for the case where
///    tmux has not yet refreshed `pane_current_command` (e.g. a pane
///    that just started), also keep panes whose `pid`'s `comm` reads
///    `opencode` via `/proc/<pid>/comm` (Linux) or `ps` (macOS).
///
/// Returns an empty vector when tmux is not available so the plugin can run
/// in environments where tmux is not installed.
pub async fn discover_opencode_panes() -> Result<Vec<OpencodePane>, DiscoveryError> {
    discover_with_tmux(Path::new("tmux")).await
}

/// Like [`discover_opencode_panes`] but lets the caller choose the `tmux`
/// binary path. Useful for tests (inject a fake `tmux` script) and for
/// non-standard installs (`$TMUX_BIN`).
pub async fn discover_with_tmux(tmux: &Path) -> Result<Vec<OpencodePane>, DiscoveryError> {
    let raws = match enumerate_panes(tmux).await {
        Ok(r) => r,
        Err(DiscoveryError::TmuxSpawn(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            // tmux isn't installed: nothing to discover.
            return Ok(Vec::new());
        }
        Err(other) => return Err(other),
    };
    let mut out = Vec::new();
    for r in raws {
        if !is_opencode_comm(&r.pane_current_command) {
            // pane_current_command is the source of truth: tmux
            // refreshes it on the order of milliseconds, so by the
            // time a pane exists the field is up to date.
            // Fall back to reading the pid's comm for the very
            // narrow window where tmux has not yet refreshed
            // (e.g. a pane that just exec'd opencode).
            if let Some(comm) = read_opencode_comm(r.pid).await {
                if is_opencode_comm(&comm) {
                    out.push(OpencodePane {
                        tmux_session: r.session,
                        pane_id: r.pane_id,
                        pane_pid: r.pid,
                        working_dir: r.working_dir,
                    });
                }
            }
            continue;
        }
        out.push(OpencodePane {
            tmux_session: r.session,
            pane_id: r.pane_id,
            pane_pid: r.pid,
            working_dir: r.working_dir,
        });
    }
    Ok(out)
}

/// Enumerate every tmux pane on the host, regardless of which process
/// is running in it. The watcher uses this to detect "opencode
/// finished but the pane is still there" — a case where
/// [`discover_with_tmux`] will drop the pane (its
/// `pane_current_command` is no longer `opencode`) but the watcher
/// must emit `session.finished` for the previously discovered
/// session.
///
/// Returns an empty vector when tmux is not available.
pub(crate) async fn enumerate_panes(tmux: &Path) -> Result<Vec<RawPane>, DiscoveryError> {
    let raw = match run_tmux_list_panes(tmux).await {
        Ok(s) => s,
        Err(DiscoveryError::TmuxSpawn(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(other) => return Err(other),
    };
    Ok(parse_tmux_list_panes(&raw))
}

/// Build the `pane_key` used to identify a tmux pane in the watcher's
/// diff map. Format: `<tmux_session>:<pane_id>`, e.g. `dev:%0`.
pub fn pane_key_from(tmux_session: &str, pane_id: &str) -> String {
    format!("{tmux_session}:{pane_id}")
}

async fn run_tmux_list_panes(tmux: &Path) -> Result<String, DiscoveryError> {
    let out = tokio::process::Command::new(tmux)
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name} #{pane_id} #{pane_pid} #{pane_current_path} #{pane_current_command}",
        ])
        .output()
        .await
        .map_err(DiscoveryError::TmuxSpawn)?;
    if !out.status.success() {
        return Err(DiscoveryError::TmuxFailed {
            status: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Parse the line-oriented output of `tmux list-panes -a -F ...`.
///
/// Malformed lines (wrong number of fields, non-numeric pid) are skipped.
/// The path and command fields are taken verbatim — callers may want to
/// canonicalize. The command field is the LAST field and is allowed to
/// contain spaces (e.g. `"bash -c 'opencode run ...'"`), so the parser
/// uses `splitn(5, ' ')` rather than `split_whitespace`.
pub(crate) fn parse_tmux_list_panes(output: &str) -> Vec<RawPane> {
    output
        .lines()
        .filter_map(|line| {
            let mut it = line.splitn(5, ' ');
            let session = it.next()?.to_string();
            let pane_id = it.next()?.to_string();
            let pid_str = it.next()?;
            let pid: u32 = pid_str.parse().ok()?;
            let working_dir = PathBuf::from(it.next()?);
            let pane_current_command = it.next()?.to_string();
            Some(RawPane {
                session,
                pane_id,
                pid,
                working_dir,
                pane_current_command,
            })
        })
        .collect()
}

/// Return true iff `comm` is the basename of the `opencode` binary.
///
/// On Linux `/proc/<pid>/comm` is exact. On macOS `ps -o comm=` is
/// case-insensitive relative to the kernel — we match case-insensitively
/// to be safe.
pub fn is_opencode_comm(comm: &str) -> bool {
    let trimmed = comm.trim();
    if trimmed.eq_ignore_ascii_case("opencode") {
        return true;
    }
    // Some shells or wrappers show `opencode (some/args)`. Trim at first space.
    let first = trimmed.split_whitespace().next().unwrap_or(trimmed);
    first.eq_ignore_ascii_case("opencode")
}

/// Read the foreground-process `comm` for `pid`, or `None` if the
/// process does not exist or its comm is unreadable.
async fn read_opencode_comm(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        read_comm_from_proc(Path::new("/proc"), pid).await
    }
    #[cfg(target_os = "macos")]
    {
        read_comm_via_ps(pid).await
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        None
    }
}

/// Linux-only helper: read `<proc_root>/<pid>/comm`. Returns `None` for
/// missing-process / unreadable-file cases so the discovery loop stays
/// linear in its `match` chain.
#[cfg(target_os = "linux")]
pub(crate) async fn read_comm_from_proc(proc_root: &Path, pid: u32) -> Option<String> {
    let path = proc_root.join(pid.to_string()).join("comm");
    let bytes = tokio::fs::read(&path).await.ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// macOS-only helper: `ps -p <pid> -o comm=` returns the process name on
/// stdout. Returns `None` if the process is gone or `ps` fails.
#[cfg(target_os = "macos")]
pub(crate) async fn read_comm_via_ps(pid: u32) -> Option<String> {
    let out = tokio::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn parse_tmux_list_panes_parses_valid_output() {
        let out = "mysession %0 12345 /home/u/proj bash\nanothersession %1 67890 /tmp zsh\n";
        let panes = parse_tmux_list_panes(out);
        assert_eq!(panes.len(), 2);
        assert_eq!(
            panes[0],
            RawPane {
                session: "mysession".into(),
                pane_id: "%0".into(),
                pid: 12345,
                working_dir: PathBuf::from("/home/u/proj"),
                pane_current_command: "bash".into(),
            }
        );
        assert_eq!(
            panes[1],
            RawPane {
                session: "anothersession".into(),
                pane_id: "%1".into(),
                pid: 67890,
                working_dir: PathBuf::from("/tmp"),
                pane_current_command: "zsh".into(),
            }
        );
    }

    #[test]
    fn parse_tmux_list_panes_handles_pane_current_command_with_spaces() {
        // tmux pane_current_command can contain spaces
        // (e.g. `opencode run 'msg'`). The parser must use
        // `splitn(5, ' ')` so the trailing command field is not split.
        let out = "mysession %0 12345 /tmp opencode run 'msg'\n";
        let panes = parse_tmux_list_panes(out);
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].session, "mysession");
        assert_eq!(panes[0].pane_id, "%0");
        assert_eq!(panes[0].pid, 12345);
        assert_eq!(panes[0].working_dir, PathBuf::from("/tmp"));
        assert_eq!(panes[0].pane_current_command, "opencode run 'msg'");
    }

    #[test]
    fn parse_tmux_list_panes_handles_pane_current_command_with_shell_command() {
        // When the pane's foreground is `bash -c 'opencode run ...'`,
        // the command field can contain both spaces and quotes. The
        // parser must keep the whole command in one field.
        let out = "mysession %0 12345 /tmp bash -c 'opencode run ...'\n";
        let panes = parse_tmux_list_panes(out);
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].pane_current_command, "bash -c 'opencode run ...'");
    }

    #[test]
    fn parse_tmux_list_panes_returns_empty_for_empty_input() {
        assert!(parse_tmux_list_panes("").is_empty());
        assert!(parse_tmux_list_panes("\n\n\n").is_empty());
    }

    #[test]
    fn parse_tmux_list_panes_skips_malformed_lines() {
        let out = "ok %0 12345 /tmp bash\nbad-line\n%1 67890 /tmp zsh\n";
        let panes = parse_tmux_list_panes(out);
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].pane_id, "%0");
    }

    #[test]
    fn is_opencode_comm_matches_opencode_exact() {
        assert!(is_opencode_comm("opencode"));
        assert!(is_opencode_comm("opencode\n"));
    }

    #[test]
    fn is_opencode_comm_matches_case_insensitively() {
        assert!(is_opencode_comm("OpenCode"));
        assert!(is_opencode_comm("OPENCODE"));
    }

    #[test]
    fn is_opencode_comm_truncates_at_first_space() {
        assert!(is_opencode_comm("opencode --watch /tmp/proj"));
    }

    #[test]
    fn is_opencode_comm_rejects_other_names() {
        assert!(!is_opencode_comm("bash"));
        assert!(!is_opencode_comm("zsh"));
        assert!(!is_opencode_comm("node"));
        assert!(!is_opencode_comm(""));
        assert!(!is_opencode_comm("opencodex"));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn read_comm_from_proc_reads_fake_proc() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pid_dir = dir.path().join("4242");
        tokio::fs::create_dir_all(&pid_dir).await.expect("mkdir");
        tokio::fs::write(pid_dir.join("comm"), "opencode\n")
            .await
            .expect("write");
        let comm = read_comm_from_proc(dir.path(), 4242).await;
        assert_eq!(comm.as_deref(), Some("opencode\n"));
    }
}
