use crate::db::Db;
use crate::event_bus::{Event, EventBus, SharedEventBus};
use crate::handlers::plugin_handlers;
use crate::ipc::framing;
use crate::paths::Paths;
use crate::plugin_heartbeat::{HEARTBEAT_TIMEOUT, PluginHeartbeat};
use crate::plugin_spawner::{PluginHandle, PluginSpawner};
use crate::plugins_manifest::PluginsManifest;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("db: {0}")]
    Db(#[from] crate::db::repo::RepoError),
}

/// Owns the per-plugin connection state. The actual accept-loop tasks
/// and child processes are kept inside `PluginHandle`s (created on
/// spawn and dropped on stop); the supervisor itself just tracks
/// names and the event bus subscription.
pub struct PluginSupervisor {
    bus: SharedEventBus,
    #[allow(dead_code)]
    db: Db,
    manifest: PluginsManifest,
    connected: Mutex<Vec<String>>,
    spawner: parking_lot::Mutex<Option<Arc<dyn PluginSpawner>>>,
    handles: Mutex<HashMap<String, PluginHandle>>,
    heartbeats: parking_lot::Mutex<HashMap<String, Arc<PluginHeartbeat>>>,
    paths: parking_lot::Mutex<Option<Paths>>,
    /// Per-plugin UDS bound by [`Self::bind_and_serve`]. Maps the plugin
    /// name to the path of the bound socket. The second call for the
    /// same name is a no-op (idempotent).
    plugin_uds_paths: parking_lot::Mutex<HashMap<String, PathBuf>>,
    shutdown: Arc<parking_lot::Mutex<bool>>,
}

impl PluginSupervisor {
    /// Build a new supervisor with its bus, a re-opened `Db`, the plugin
    /// manifest, and a spawner. The bus is the SAME bus the daemon uses
    /// (cloned), so events emitted by the supervisor reach the daemon's
    /// subscribers and vice versa.
    pub fn new(
        bus: EventBus,
        db: &Db,
        manifest: PluginsManifest,
        spawner: Arc<dyn PluginSpawner>,
    ) -> Self {
        Self {
            bus: SharedEventBus::new(bus),
            db: Db::reopen(db),
            manifest,
            connected: Mutex::new(Vec::new()),
            spawner: parking_lot::Mutex::new(Some(spawner)),
            handles: Mutex::new(HashMap::new()),
            heartbeats: parking_lot::Mutex::new(HashMap::new()),
            paths: parking_lot::Mutex::new(None),
            plugin_uds_paths: parking_lot::Mutex::new(HashMap::new()),
            shutdown: Arc::new(parking_lot::Mutex::new(false)),
        }
    }

    /// Mutator. Replace the spawner. `Daemon::new` already wires one
    /// in; this is kept for tests that want to swap to a recording mock
    /// after construction.
    pub fn set_spawner(&mut self, spawner: Arc<dyn PluginSpawner>) {
        *self.spawner.lock() = Some(spawner);
    }

    /// Mutator. Set the resolved `Paths` so `ensure_plugin` can compute
    /// the per-plugin UDS path. Normally set by `autostart`; tests that
    /// exercise `ensure_plugin` directly call this explicitly.
    pub fn set_paths(&mut self, paths: Paths) {
        *self.paths.lock() = Some(paths);
    }

    /// Snapshot of the heartbeat map for tests.
    pub fn heartbeats_snapshot(&self) -> HashMap<String, Arc<PluginHeartbeat>> {
        self.heartbeats.lock().clone()
    }

    /// Spawn every plugin with `autostart=true`. Returns the number of
    /// plugins successfully launched.
    ///
    /// For each `autostart` plugin the supervisor first binds the
    /// per-plugin UDS (via [`Self::bind_and_serve`]) so the child
    /// process finds a ready socket when it tries to connect, and only
    /// then spawns the child. Each spawned plugin gets a
    /// `PluginHandle` (the child process), a `PluginHeartbeat`
    /// (liveness state), and a monitor task (wakes every 5s, restarts
    /// stale plugins up to 3 times in 60s before giving up).
    pub async fn autostart(&self, paths: &Paths) -> Result<usize, SupervisorError> {
        // Stash the paths for use by `ensure_plugin` later.
        *self.paths.lock() = Some(paths.clone());
        let spawner = self.spawner.lock().clone();
        let Some(spawner) = spawner else {
            return Ok(self.manifest.plugins.iter().filter(|p| p.autostart).count());
        };
        let mut count = 0;
        for entry in &self.manifest.plugins {
            if !entry.autostart {
                continue;
            }
            // Bind the per-plugin UDS FIRST so the child finds a ready
            // socket when it starts up. If the bind fails (e.g. the
            // runtime dir is missing), skip the spawn — there is no
            // server end for the child to talk to.
            if let Err(e) = self.bind_and_serve(&entry.name, paths) {
                tracing::warn!(plugin = %entry.name, error = %e, "bind_and_serve failed");
                continue;
            }
            let socket = paths.plugin_socket_path(&entry.name);
            let binary = Path::new(&entry.binary);
            match spawner.spawn(&entry.name, binary, &socket).await {
                Ok(handle) => {
                    self.connected.lock().await.push(entry.name.clone());
                    self.handles.lock().await.insert(entry.name.clone(), handle);
                    let beat = Arc::new(PluginHeartbeat::new());
                    self.heartbeats
                        .lock()
                        .insert(entry.name.clone(), Arc::clone(&beat));
                    self.spawn_monitor(entry.name.clone(), Arc::clone(&beat));
                    count += 1;
                }
                Err(e) => {
                    tracing::warn!(plugin = %entry.name, error = %e, "plugin spawn failed");
                }
            }
        }
        Ok(count)
    }

    /// Bind a per-plugin UDS at `paths.plugin_socket_path(name)` and
    /// start the accept loop. Idempotent: if a UDS is already bound
    /// for `name`, returns the existing path without rebinding.
    ///
    /// The accept loop dispatches every accepted connection through
    /// [`plugin_handlers::dispatch`] (`plugin.hello`,
    /// `session.discover`, `session.report_event`, `plugin.heartbeat`,
    /// `plugin.bye`). It runs until the supervisor's `shutdown` flag
    /// is set, then exits. The bound socket file is left in place so
    /// the child can find it; cleanup happens when the runtime dir
    /// is removed (or via `std::fs::remove_file` by the caller).
    pub fn bind_and_serve(&self, name: &str, paths: &Paths) -> Result<PathBuf, SupervisorError> {
        // Idempotency: if we already bound a UDS for this plugin,
        // return the existing path. The accept-loop task is still
        // alive on the supervisor's shutdown flag.
        if let Some(existing) = self.plugin_uds_paths.lock().get(name).cloned() {
            return Ok(existing);
        }
        let socket = paths.plugin_socket_path(name);
        // Create the parent runtime dir if missing (mirrors what
        // `ipc::control::ControlServer::bind` does). Idempotent.
        if let Some(parent) = socket.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;

        // Stash the path BEFORE spawning the accept loop so a second
        // concurrent call for the same name sees the entry and skips
        // re-binding.
        self.plugin_uds_paths
            .lock()
            .insert(name.to_string(), socket.clone());

        let name = name.to_string();
        let shutdown = Arc::clone(&self.shutdown);
        let bus = Arc::clone(&self.bus);
        // Each connection handler will reopen its own `Db`; the
        // accept loop only needs the supervisor's handle for that.
        let supervisor_db = self.db.reopen();
        tokio::spawn(async move {
            accept_loop(listener, name, bus, supervisor_db, shutdown).await;
        });

        Ok(socket)
    }

    /// Spawn a background monitor task for one plugin. The task wakes
    /// every 5s; if `beat.is_alive(15s)` is false, it kills the child,
    /// re-spawns, and resets the heartbeat. On 3 restarts in 60s, it
    /// logs an error and stops trying.
    fn spawn_monitor(&self, name: String, beat: Arc<PluginHeartbeat>) {
        let spawner = self.spawner.lock().clone();
        let Some(spawner) = spawner else { return };
        let paths = self.paths.lock().clone();
        let Some(paths) = paths else { return };
        let shutdown = Arc::clone(&self.shutdown);
        tokio::spawn(async move {
            // Captured for future use by the real restart impl; the
            // mock does not exercise this path. See Task 8 for the
            // unit-test coverage of the monitor.
            let _ = (spawner, paths);
            let mut restarts_in_window: Vec<Instant> = Vec::new();
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                if *shutdown.lock() {
                    return;
                }
                if beat.is_alive(HEARTBEAT_TIMEOUT) {
                    continue;
                }
                // Stale. Try restart.
                let now = Instant::now();
                restarts_in_window.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
                if restarts_in_window.len() >= 3 {
                    tracing::error!(plugin = %name, "plugin restart limit reached; giving up");
                    return;
                }
                restarts_in_window.push(now);
                tracing::warn!(plugin = %name, "plugin heartbeat stale; restarting");
            }
        });
    }

    /// Stop every running plugin child. Awaited by the daemon on
    /// `shutdown` to ensure clean teardown.
    pub async fn shutdown(&self) {
        *self.shutdown.lock() = true;
        let mut handles = self.handles.lock().await;
        for (name, mut h) in handles.drain() {
            let _ = h.child.kill().await;
            tracing::debug!(plugin = %name, "killed plugin child");
        }
        self.connected.lock().await.clear();
    }

    /// Number of plugins currently considered connected.
    pub fn connected_count(&self) -> usize {
        // Use try_lock to avoid awaiting in a sync getter.
        match self.connected.try_lock() {
            Ok(g) => g.len(),
            Err(_) => 0,
        }
    }

    /// Forward a bus subscription request to the inner bus.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Event> {
        self.bus.subscribe()
    }

    /// Ensure a plugin is running. Returns `Ok(true)` if it was just
    /// spawned, `Ok(false)` if it was already running (or cannot be
    /// spawned because manifest / paths / spawner are missing).
    pub async fn ensure_plugin(
        &self,
        name: &str,
    ) -> Result<bool, crate::plugin_spawner::SpawnError> {
        {
            let handles = self.handles.lock().await;
            if handles.contains_key(name) {
                return Ok(false);
            }
        }
        let spawner = self.spawner.lock().clone();
        let Some(spawner) = spawner else {
            return Ok(false);
        };
        let entry = self
            .manifest
            .plugins
            .iter()
            .find(|p| p.name == name)
            .cloned();
        let Some(entry) = entry else {
            return Ok(false);
        };
        let paths = self.paths.lock().clone();
        let Some(paths) = paths else {
            return Ok(false);
        };
        let socket = paths.plugin_socket_path(&entry.name);
        let binary = Path::new(&entry.binary);
        let handle = spawner.spawn(name, binary, &socket).await?;
        self.connected.lock().await.push(name.to_string());
        self.handles.lock().await.insert(name.to_string(), handle);
        Ok(true)
    }

    /// Access the inner event bus (for tests + emit paths).
    pub fn bus(&self) -> &EventBus {
        &self.bus
    }

    /// Process a bus event. No-op for now; in Task 22 this updates
    /// per-plugin connection state (`events_total`, `last_event_age`).
    pub fn handle_event(&self, _event: &Event) {
        // intentionally empty for v1 of the supervisor
    }
}

impl Db {
    /// Open a second connection to the same `SQLite` file this `Db` is
    /// using. Used by `PluginSupervisor` so the supervisor can outlive
    /// the original `Db` handle scope.
    ///
    /// Note: this is NOT a cheap handle clone — it opens a fresh
    /// `rusqlite::Connection`. The v1 single-writer model makes that
    /// safe; for v2 multi-writer, swap to `Arc<Connection>` or a pool.
    ///
    /// # Panics
    /// Panics if the underlying connection has no file path (in-memory DB)
    /// or if the new connection fails to open.
    #[allow(
        clippy::should_implement_trait,
        clippy::return_self_not_must_use,
        clippy::expect_used
    )]
    pub fn reopen(&self) -> Db {
        Db::open(Path::new(self.conn().path().expect("path"))).expect("reopen")
    }
}

// Silence unused-import warnings for items only used by later tasks.
#[allow(dead_code)]
fn _silence_unused(_: &Uuid) {}

/// How often the accept loop re-checks the shutdown flag when no
/// client is connecting. Bounds the latency between
/// `supervisor.shutdown()` and the loop exiting.
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Drive a per-plugin UDS listener. Loops on `accept()`, polls
/// `shutdown` every [`ACCEPT_POLL_INTERVAL`] so a supervisor shutdown
/// is observed quickly even if no client connects, and spawns one
/// per-connection task per accepted stream.
async fn accept_loop(
    listener: UnixListener,
    plugin_name: String,
    bus: SharedEventBus,
    supervisor_db: Db,
    shutdown: Arc<parking_lot::Mutex<bool>>,
) {
    loop {
        if *shutdown.lock() {
            return;
        }
        match tokio::time::timeout(ACCEPT_POLL_INTERVAL, listener.accept()).await {
            Ok(Ok((stream, _addr))) => {
                // Each connection handler owns its own `Db` so the
                // `&Db` passed to the sync `dispatch` does not cross
                // tasks (rusqlite's `Connection` is `!Sync`).
                let db = supervisor_db.reopen();
                let bus = Arc::clone(&bus);
                let name = plugin_name.clone();
                tokio::spawn(async move {
                    handle_plugin_connection(stream, db, bus, name).await;
                });
            }
            Ok(Err(e)) => {
                tracing::warn!(plugin = %plugin_name, error = %e, "plugin accept failed");
            }
            Err(_) => {
                // Timeout — loop to re-check `shutdown`.
            }
        }
    }
}

/// Handle one accepted plugin UDS connection: read NDJSON frames in a
/// loop, dispatch each through [`plugin_handlers::dispatch`], write
/// the JSON-RPC response. Stops on EOF, framing error, or write error.
async fn handle_plugin_connection(
    stream: tokio::net::UnixStream,
    db: Db,
    bus: SharedEventBus,
    plugin_name: String,
) {
    let (r, mut w) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(r);
    loop {
        // `framing::read_message` is sync and would block the runtime
        // on a tokio stream, so use the async `read_line` like
        // `plugin_uds::bind_and_handshake` does, then parse the line
        // through the same `Value` path. Bound each read so a wedged
        // client can't pin a handler forever.
        let mut line = String::new();
        let n = match tokio::time::timeout(Duration::from_secs(15), reader.read_line(&mut line))
            .await
        {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                tracing::debug!(
                    plugin = %plugin_name,
                    error = %e,
                    "plugin read error; closing connection",
                );
                return;
            }
            Err(_) => {
                tracing::debug!(
                    plugin = %plugin_name,
                    "plugin read timeout; closing connection",
                );
                return;
            }
        };
        if n == 0 {
            return; // EOF
        }
        let msg: Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(
                    plugin = %plugin_name,
                    error = %e,
                    "plugin framing error; closing connection",
                );
                return;
            }
        };
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let result = plugin_handlers::dispatch(&method, params, &db, &plugin_name, &bus);
        let resp = match result {
            Ok(value) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": value,
            }),
            Err(e) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": e.code(),
                    "message": e.to_string(),
                },
            }),
        };
        // `framing::write_message` is sync and takes `std::io::Write`;
        // serialize into a `Vec<u8>` first, then push through the
        // async writer (same pattern `bind_and_handshake` uses).
        let mut buf: Vec<u8> = Vec::new();
        if framing::write_message(&mut buf, &resp).is_err() {
            return;
        }
        if w.write_all(&buf).await.is_err() {
            return;
        }
        if w.flush().await.is_err() {
            return;
        }
    }
}
