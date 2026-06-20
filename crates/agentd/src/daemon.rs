use crate::control_client::ControlClient;
use crate::db::Db;
use crate::event_bus::EventBus;
use crate::ipc::control::ControlServer;
use crate::paths::Paths;
use crate::plugin_spawner::PluginSpawner;
use crate::plugin_supervisor::PluginSupervisor;
use crate::plugins_manifest::PluginsManifest;
use crate::tmux::Tmux;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// Migration application failure.
    #[error("migrations: {0}")]
    Migrations(#[from] crate::db::migrations::MigrationError),
    /// Plugin supervisor failure.
    #[error("supervisor: {0}")]
    Supervisor(#[from] crate::plugin_supervisor::SupervisorError),
    /// Plugin spawn failure.
    #[error("spawn: {0}")]
    Spawn(#[from] crate::plugin_spawner::SpawnError),
    /// Control UDS bind/serve failure.
    #[error("control: {0}")]
    Control(#[from] crate::ipc::control::ControlError),
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
    pub tmux: Arc<dyn Tmux>,
    /// Plugin supervisor (manifest + spawner + connection state).
    pub supervisor: PluginSupervisor,
    /// Flag set by external callers (CLI, signal handler) to ask the
    /// daemon to shut down.
    pub shutdown: Arc<AtomicBool>,
}

impl Daemon {
    /// Build a `Daemon` from its parts. Does no I/O — pure constructor.
    ///
    /// `spawner` is required: the supervisor uses it to launch plugin
    /// children. Tests pass a `MockPluginSpawner`; production passes a
    /// `RealPluginSpawner`.
    pub fn new(
        paths: Paths,
        db: Db,
        bus: EventBus,
        tmux: Arc<dyn Tmux>,
        manifest: PluginsManifest,
        spawner: Arc<dyn PluginSpawner>,
    ) -> Self {
        let supervisor = PluginSupervisor::new(bus.clone(), &db, manifest, spawner);
        Self {
            paths,
            db,
            bus,
            tmux,
            supervisor,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Cloneable handle for external callers (CLI, signal handler) that
    /// need to ask the daemon to shut down.
    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Boot the daemon through steps 1-7 of the boot sequence, then
    /// idle on the event bus until [`shutdown_handle`](Self::shutdown_handle)
    /// is set. On shutdown, abort the control UDS task and tear down the
    /// plugin supervisor.
    #[allow(unused_mut)]
    pub async fn run(mut self) -> Result<(), DaemonError> {
        // Step 1: flock.
        let _flock = acquire_flock(&self.paths.daemon_lock_path)?;
        // Step 2: mkdir 0700.
        std::fs::create_dir_all(&self.paths.runtime_dir)?;
        std::fs::set_permissions(
            &self.paths.runtime_dir,
            std::fs::Permissions::from_mode(0o700),
        )?;
        // Step 3: migrations (already done by caller, but be safe).
        crate::db::migrations::run(&self.db)?;
        // Step 4: tombstone GC.
        tombstone_gc(&self.db)?;
        // Step 5: restart-respawn. Stash paths on the supervisor first so
        // `ensure_plugin` can resolve per-plugin UDS paths.
        self.supervisor.set_paths(self.paths.clone());
        let spawned = restart_respawn(&self.db, &self.supervisor).await?;
        tracing::info!(spawned, "restart_respawn complete");
        // Step 6: bind control UDS.
        let registry = Arc::new(crate::handlers::subscriber_registry::SubscriberRegistry::new());
        let mut bus_rx = self.bus.subscribe();
        let registry_for_task = Arc::clone(&registry);
        tokio::spawn(async move {
            loop {
                match bus_rx.recv().await {
                    Ok(event) => registry_for_task.broadcast(&event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "bus forwarder lagged; continuing");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        let control = ControlServer::bind(&self.paths.control_socket_path)?;
        let paths_for_handler = self.paths.clone();
        let tmux_for_handler = Arc::clone(&self.tmux);
        let registry_for_handler = Arc::clone(&registry);
        let bus_for_handler = self.bus.clone();
        let control_handle = tokio::spawn(async move {
            control
                .serve(move |stream| {
                    crate::handlers::router::handle_client(
                        stream,
                        &paths_for_handler,
                        &*tmux_for_handler,
                        &registry_for_handler,
                        &bus_for_handler,
                    );
                })
                .await;
        });
        // Step 7: autostart plugins.
        let started = self.supervisor.autostart(&self.paths).await?;
        tracing::info!(started, "autostart complete");
        // Idle: subscribe to bus + sleep until shutdown.
        let mut rx = self.bus.subscribe();
        loop {
            if self.shutdown.load(Ordering::SeqCst) {
                break;
            }
            tokio::select! {
                _ = rx.recv() => {},
                () = tokio::time::sleep(std::time::Duration::from_millis(200)) => {},
            }
        }
        control_handle.abort();
        self.supervisor.shutdown().await;
        // Tear down runtime artifacts so a re-bind on next start does not
        // race with a stale socket. Best-effort: ignore errors (file may
        // already be gone if the OS cleaned it up).
        let _ = std::fs::remove_file(&self.paths.control_socket_path);
        let _ = std::fs::remove_file(&self.paths.daemon_lock_path);
        let _ = std::fs::remove_file(self.paths.daemon_pid_path());
        Ok(())
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

/// For every non-finished session, look up `metadata["plugin"]` and
/// ensure the owning plugin is connected. Returns the number of
/// plugins spawned. The plugin's own scan loop will pick up the
/// session naturally; no synthetic RPC is sent.
pub async fn restart_respawn(db: &Db, supervisor: &PluginSupervisor) -> Result<usize, DaemonError> {
    use crate::db::repo::SessionRepo;
    use agentd_protocol::SessionStatus;
    let active = SessionRepo::new(db)
        .list_non_finished()
        .map_err(DaemonError::Db)?;
    let mut needed: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in active {
        if s.status == SessionStatus::Finished {
            continue;
        }
        if let Some(p) = s.metadata.get("plugin").and_then(serde_json::Value::as_str) {
            needed.insert(p.to_string());
        }
    }
    let mut spawned = 0;
    for name in needed {
        if supervisor.ensure_plugin(&name).await? {
            spawned += 1;
        }
    }
    Ok(spawned)
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
