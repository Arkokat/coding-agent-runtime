use parking_lot::Mutex;
use std::time::{Duration, Instant};

/// After this long without a heartbeat, the supervisor's monitor task
/// considers the plugin stale and restarts it. 5s check interval × 3
/// retries = 15s.
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

/// Per-plugin heartbeat state. Owned by the supervisor and shared
/// (via `Arc`) with the per-plugin UDS server task (which calls
/// `touch()`) and the monitor task (which calls `is_alive`).
///
/// In Task 8 this struct grows `events_total` / `invalid_total` /
/// `last_event_age` atomic counters and a `handle_heartbeat(&h, &body)`
/// wire function.
pub struct PluginHeartbeat {
    /// Time of the last heartbeat. Updated by `touch`.
    pub last: Mutex<Instant>,
}

impl PluginHeartbeat {
    /// Build a fresh heartbeat with `last = Instant::now()`.
    pub fn new() -> Self {
        Self {
            last: Mutex::new(Instant::now()),
        }
    }

    /// Refresh `last` to `Instant::now()`. Called from the per-plugin
    /// UDS server when a `plugin.heartbeat` notification arrives.
    pub fn touch(&self) {
        *self.last.lock() = Instant::now();
    }

    /// `true` if the last heartbeat was within `timeout`.
    pub fn is_alive(&self, timeout: Duration) -> bool {
        self.last.lock().elapsed() < timeout
    }
}

impl Default for PluginHeartbeat {
    fn default() -> Self {
        Self::new()
    }
}
