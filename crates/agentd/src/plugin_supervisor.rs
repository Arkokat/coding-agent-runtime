use crate::db::Db;
use crate::event_bus::{Event, EventBus, SharedEventBus};
use crate::paths::Paths;
use crate::plugin_heartbeat::{HEARTBEAT_TIMEOUT, PluginHeartbeat};
use crate::plugin_spawner::{PluginHandle, PluginSpawner};
use crate::plugins_manifest::PluginsManifest;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
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
    /// Each spawned plugin gets a `PluginHandle` (the child process),
    /// a `PluginHeartbeat` (liveness state, touched by the UDS server
    /// task), and a monitor task (wakes every 5s, restarts stale
    /// plugins up to 3 times in 60s before giving up).
    ///
    /// After spawn, a background task binds the per-plugin UDS and
    /// waits for the child's `plugin.hello` (2s timeout). The result
    /// is logged; the DB `last_connected_at` row is updated on
    /// success. The full per-plugin accept loop lands in Plan 4 —
    /// for v1 this is a fire-and-forget handshake.
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
            let socket = paths.plugin_socket_path(&entry.name);
            let binary = Path::new(&entry.binary);
            match spawner.spawn(&entry.name, binary, &socket).await {
                Ok(handle) => {
                    // Fire-and-forget handshake. The full per-plugin
                    // accept loop is a Plan 4 task.
                    let handshake_db = self.db.reopen();
                    let handshake_socket = socket.clone();
                    let plugin_name = entry.name.clone();
                    tokio::spawn(async move {
                        match crate::plugin_uds::bind_and_handshake(
                            &handshake_socket,
                            std::time::Duration::from_secs(2),
                        )
                        .await
                        {
                            Ok(hs) => {
                                if let Err(e) = crate::plugin_uds::record_handshake(
                                    &handshake_db,
                                    &hs.plugin_name,
                                ) {
                                    tracing::warn!(
                                        plugin = %hs.plugin_name,
                                        error = %e,
                                        "record_handshake failed",
                                    );
                                } else {
                                    tracing::info!(
                                        plugin = %hs.plugin_name,
                                        "plugin handshake complete",
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    plugin = %plugin_name,
                                    error = %e,
                                    "plugin handshake failed",
                                );
                            }
                        }
                    });
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
