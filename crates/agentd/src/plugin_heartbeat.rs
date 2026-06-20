use parking_lot::Mutex;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// How often the supervisor's per-plugin UDS server expects a
/// `plugin.heartbeat` notification from a healthy plugin. Also the
/// cadence at which the monitor task checks liveness. The plugin is
/// given this value during `plugin.hello` so it can pace its own pings.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// After this long without a heartbeat, the supervisor's monitor task
/// considers the plugin stale and restarts it. With [`HEARTBEAT_INTERVAL`]
/// of 5s, 15s is roughly 3 missed beats — long enough to ride out a
/// scheduler hiccup, short enough that a wedged plugin is noticed
/// quickly.
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

/// Per-plugin heartbeat state. Owned by the supervisor and shared
/// (via `Arc`) with the per-plugin UDS server task (which calls
/// `touch()` / `handle_heartbeat`) and the monitor task (which calls
/// `is_alive`).
///
/// `last` records the wall-clock time of the most recent heartbeat,
/// regardless of whether that heartbeat carried counters. The atomic
/// counters (`events_total`, `invalid_total`, `last_event_age`) carry
/// the last reported counts from the plugin's `plugin.heartbeat` body
/// and are exposed so the supervisor and (later) status reporters can
/// read them without going through the plugin.
pub struct PluginHeartbeat {
    /// Time of the last heartbeat. Updated by `touch` (and therefore
    /// by `handle_heartbeat`). Guarded by a `parking_lot::Mutex` so the
    /// `Instant` write/read is consistent on all platforms.
    pub last: Mutex<Instant>,

    /// Cumulative count of events the plugin reports it has emitted.
    /// Refreshed from the `events_total` field of each
    /// `plugin.heartbeat` body. Monotonically non-decreasing over the
    /// life of the plugin (the plugin is the authority).
    pub events_total: AtomicU64,

    /// Cumulative count of events the plugin reports it has rejected
    /// as invalid. Refreshed from the `invalid_total` field of each
    /// `plugin.heartbeat` body.
    pub invalid_total: AtomicU64,

    /// Seconds since the plugin last processed an event, as reported
    /// in the most recent `plugin.heartbeat` body that carried the
    /// `last_event_age_secs` field. Updated lazily — only when the
    /// plugin actually sends it — so a stale value here means the
    /// plugin didn't include it on its last ping, not that the value
    /// itself is stale.
    pub last_event_age: AtomicU64,
}

impl PluginHeartbeat {
    /// Build a fresh heartbeat with `last = Instant::now()` and all
    /// counters at zero.
    pub fn new() -> Self {
        Self {
            last: Mutex::new(Instant::now()),
            events_total: AtomicU64::new(0),
            invalid_total: AtomicU64::new(0),
            last_event_age: AtomicU64::new(0),
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

/// Update counters from a `plugin.heartbeat` body, then refresh
/// `last`. Missing fields are treated as "no change" (the count stays
/// where it was), so a partial payload — e.g. one that omits
/// `invalid_total` — does not silently zero a counter.
///
/// Called by the per-plugin UDS server when a `plugin.heartbeat`
/// notification arrives.
pub fn handle_heartbeat(h: &PluginHeartbeat, body: &Value) {
    if let Some(n) = body.get("events_total").and_then(Value::as_u64) {
        h.events_total.store(n, Ordering::SeqCst);
    }
    if let Some(n) = body.get("invalid_total").and_then(Value::as_u64) {
        h.invalid_total.store(n, Ordering::SeqCst);
    }
    h.touch();
}
