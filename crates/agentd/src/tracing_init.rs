//! Initialize the global `tracing` subscriber so the daemon and its
//! plugins can be debugged in production deployments.
//!
//! Behavior:
//! - If `AGENTD_LOG_FILE` is set in the environment, tracing is written
//!   to that file (parent directories are created on demand). ANSI
//!   color codes are suppressed because files do not render them.
//! - Otherwise, tracing is written to the user-visible default path
//!   `$XDG_STATE_HOME/agentd/daemon.log` (with `$HOME/.local/state` and
//!   `/tmp` fallbacks for hosts that do not set `XDG_STATE_HOME`). The
//!   resolved path is also written to
//!   `$XDG_STATE_HOME/agentd/daemon.log.path` so smoke scripts and
//!   humans can `cat` the right file even if they did not set the env
//!   var explicitly.
//! - If even the fallback path cannot be opened, the subscriber is
//!   silently dropped (init is best-effort; the daemon must still boot
//!   if the log directory is read-only or full).
//!
//! `init` installs a global subscriber via `try_init` — it is safe to
//! call more than once in a process; subsequent calls are no-ops.

use std::io::IsTerminal;
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt};

/// Where to write tracing output. Used by [`init`] when
/// `AGENTD_LOG_FILE` is not set.
fn default_log_path() -> PathBuf {
    if let Ok(xdg_state) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(xdg_state).join("agentd").join("daemon.log");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("agentd")
            .join("daemon.log");
    }
    PathBuf::from("/tmp").join("agentd").join("daemon.log")
}

/// Compute the [`EnvFilter`] to apply: respect `RUST_LOG` if set,
/// otherwise default to `error` when `quiet` is true and `info`
/// otherwise.
fn env_filter(quiet: bool) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if quiet {
            EnvFilter::new("error")
        } else {
            EnvFilter::new("info")
        }
    })
}

/// Install a file-backed subscriber that writes to `path` (creating
/// the file and any missing parent directories). Returns true if the
/// subscriber was installed.
fn install_file_subscriber(path: &std::path::Path, filter: EnvFilter) -> bool {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return false;
    };
    fmt()
        .with_env_filter(filter)
        .with_writer(file)
        .with_ansi(false)
        .try_init()
        .is_ok()
}

/// Install a stderr-backed subscriber. Honors ANSI colors when stderr
/// is a TTY.
fn install_stderr_subscriber(filter: EnvFilter) {
    let ansi = std::io::stderr().is_terminal();
    let _ = fmt().with_env_filter(filter).with_ansi(ansi).try_init();
}

/// Initialize the global tracing subscriber.
///
/// If `AGENTD_LOG_FILE` is set, tracing is written to that file.
/// Otherwise, tracing is written to a default log file under
/// `XDG_STATE_HOME` (with `$HOME/.local/state` and `/tmp` fallbacks).
/// If even the fallback file cannot be opened, tracing falls back to
/// stderr.
pub fn init(quiet: bool) {
    let filter = env_filter(quiet);

    if let Ok(path) = std::env::var("AGENTD_LOG_FILE") {
        let p = PathBuf::from(&path);
        if install_file_subscriber(&p, filter.clone()) {
            // Also publish the resolved path under
            // `XDG_STATE_HOME/agentd/daemon.log.path` so smoke scripts
            // can locate the file even when the env var was set
            // implicitly.
            if let Some(parent) = p.parent() {
                let _ = std::fs::write(parent.join("daemon.log.path"), path.as_bytes());
            }
            return;
        }
        // Could not open the requested file — fall through to default.
    }

    let default_path = default_log_path();
    if install_file_subscriber(&default_path, filter.clone()) {
        if let Some(parent) = default_path.parent() {
            let _ = std::fs::write(
                parent.join("daemon.log.path"),
                default_path.to_string_lossy().as_bytes(),
            );
        }
        return;
    }

    install_stderr_subscriber(filter);
}
