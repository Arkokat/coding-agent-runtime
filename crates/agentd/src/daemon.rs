use crate::control_client::ControlClient;
use crate::db::Db;
use crate::event_bus::EventBus;
use crate::paths::Paths;
use crate::plugins_manifest::PluginsManifest;
use crate::tmux::Tmux;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use thiserror::Error;

/// Errors that can occur while constructing or running the daemon.
#[derive(Debug, Error)]
pub enum DaemonError {
    /// Filesystem I/O failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// `SQLite` repository failure.
    #[error("db: {0}")]
    Db(#[from] crate::db::repo::RepoError),
    /// Another agentd daemon already holds the runtime lock.
    #[error("another agentd daemon is already running (lock held)")]
    LockHeld,
    /// The XDG runtime directory is missing or not writable.
    #[error("runtime dir missing or not writable")]
    MissingRuntimeDir,
}

/// RAII guard for a held flock. Drop releases the lock and unlinks the
/// file. If the process exits without dropping, the kernel releases the
/// flock on fd close.
pub struct FlockGuard {
    file: std::fs::File,
    path: PathBuf,
}

impl std::fmt::Debug for FlockGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlockGuard")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Drop for FlockGuard {
    fn drop(&mut self) {
        use nix::fcntl::{FlockArg, flock};
        use std::os::unix::io::AsRawFd;
        let _ = flock(self.file.as_raw_fd(), FlockArg::Unlock);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Acquire an exclusive non-blocking flock on `path`, creating the file
/// (and its parent directory, if missing) if it does not exist. Returns
/// the guard on success, or `LockHeld` if another process already holds
/// the lock.
pub fn acquire_flock(path: &Path) -> Result<FlockGuard, DaemonError> {
    use nix::fcntl::FlockArg;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    nix::fcntl::flock(file.as_raw_fd(), FlockArg::LockExclusiveNonblock)
        .map_err(|_| DaemonError::LockHeld)?;
    Ok(FlockGuard {
        file,
        path: path.to_path_buf(),
    })
}

/// Long-lived daemon state. Constructed by `Daemon::new`, run by
/// `Daemon::run` (added in later tasks).
pub struct Daemon {
    /// Resolved XDG paths.
    pub paths: Paths,
    /// Opened `SQLite` state database.
    pub db: Db,
    /// Shared event bus.
    pub bus: EventBus,
    /// Tmux backend (`RealTmux` in production, `MockTmux` in tests).
    pub tmux: Box<dyn Tmux>,
    /// Plugin manifest loaded at startup.
    pub manifest: PluginsManifest,
    /// Flag set by external callers (CLI, signal handler) to ask the
    /// daemon to shut down.
    pub shutdown: Arc<AtomicBool>,
}

impl Daemon {
    /// Build a `Daemon` from its parts. Does no I/O — pure constructor.
    pub fn new(
        paths: Paths,
        db: Db,
        bus: EventBus,
        tmux: Box<dyn Tmux>,
        manifest: PluginsManifest,
    ) -> Self {
        Self {
            paths,
            db,
            bus,
            tmux,
            manifest,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Cloneable handle for external callers (CLI, signal handler) that
    /// need to ask the daemon to shut down.
    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }
}

/// Delete `finished` sessions whose `finished_at` is older than 30 days,
/// plus any orphaned events. Returns the number of sessions deleted.
///
/// Called once at daemon boot, before the control UDS is bound, so
/// client queries never see a partially-GCed state.
pub fn tombstone_gc(db: &crate::db::Db) -> Result<usize, DaemonError> {
    // Delete orphaned events first (sessions about to vanish).
    db.conn()
        .execute(
            "DELETE FROM session_events WHERE session_id NOT IN (SELECT id FROM sessions)",
            [],
        )
        .map_err(crate::db::repo::RepoError::from)
        .map_err(DaemonError::Db)?;
    let n = db
        .conn()
        .execute(
            "DELETE FROM sessions
             WHERE status = 'finished'
               AND finished_at < datetime('now', '-30 days')",
            [],
        )
        .map_err(crate::db::repo::RepoError::from)
        .map_err(DaemonError::Db)?;
    Ok(n)
}

/// Ensure a daemon is running and return a connected `ControlClient`.
///
/// 1. Try to connect to the control UDS with a 100ms timeout.
/// 2. On failure, fork the current binary as `agentd daemon start --detach`
///    (the detached daemon will bind the UDS itself).
/// 3. Poll the UDS every 50ms for up to 2s.
/// 4. Return a `ControlClient` ready to call.
pub async fn ensure_daemon_running(paths: &Paths) -> Result<ControlClient, DaemonError> {
    // Step 1: try connect.
    if let Ok(client) = tokio::time::timeout(
        Duration::from_millis(100),
        ControlClient::connect(&paths.control_socket_path),
    )
    .await
    .map_err(|_| {
        DaemonError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "connect timeout",
        ))
    })? {
        return Ok(client);
    }

    // Step 2: fork detached.
    let exe = std::env::current_exe().map_err(DaemonError::Io)?;
    let status = std::process::Command::new(exe)
        .args(["daemon", "start", "--detach"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(DaemonError::Io)?;
    if !status.success() {
        return Err(DaemonError::Io(std::io::Error::other(format!(
            "daemon start --detach exited {status:?}"
        ))));
    }

    // Step 3: poll.
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(client) = ControlClient::connect(&paths.control_socket_path).await {
            return Ok(client);
        }
    }
    Err(DaemonError::Io(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "daemon did not become reachable within 2s",
    )))
}
