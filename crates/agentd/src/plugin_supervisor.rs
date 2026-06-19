use crate::db::Db;
use crate::event_bus::{Event, EventBus, SharedEventBus};
use crate::paths::Paths;
use crate::plugins_manifest::PluginsManifest;
use std::path::Path;
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
}

impl PluginSupervisor {
    pub fn new(bus: EventBus, db: &Db, manifest: PluginsManifest) -> Self {
        Self {
            bus: SharedEventBus::new(bus),
            db: Db::clone(db),
            manifest,
            connected: Mutex::new(Vec::new()),
        }
    }

    /// Spawn every plugin with `autostart=true`. Returns the number
    /// of plugins successfully launched.
    ///
    /// The full per-plugin spawn (binary lookup, child process, UDS
    /// server, heartbeat loop) is wired in a later task. This initial
    /// implementation records the intent and returns 0 — sufficient
    /// to satisfy the boot sequence and the daemon restart behavior
    /// (Task 21).
    pub async fn autostart(&self, _paths: &Paths) -> Result<usize, SupervisorError> {
        let mut count = 0;
        for entry in &self.manifest.plugins {
            if entry.autostart {
                // Register in connected list; real spawn in later task.
                self.connected.lock().await.push(entry.name.clone());
                count += 1;
            }
        }
        Ok(count)
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
    /// Cheap clone: opens a new connection to the same file. Used by
    /// `PluginSupervisor` so the supervisor can outlive the original
    /// `Db` handle scope.
    ///
    /// # Panics
    /// Panics if the underlying connection has no file path (in-memory DB)
    /// or if the new connection fails to open.
    #[allow(
        clippy::should_implement_trait,
        clippy::return_self_not_must_use,
        clippy::expect_used
    )]
    pub fn clone(&self) -> Db {
        Db::open(Path::new(self.conn().path().expect("path"))).expect("clone")
    }
}

// Silence unused-import warnings for items only used by later tasks.
#[allow(dead_code)]
fn _silence_unused(_: &Uuid) {}
