#![allow(clippy::expect_used)]

use agentd::plugin_heartbeat::{HEARTBEAT_TIMEOUT, PluginHeartbeat, handle_heartbeat};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[test]
fn fresh_heartbeat_is_alive() {
    let h = PluginHeartbeat::new();
    assert!(
        h.is_alive(HEARTBEAT_TIMEOUT),
        "fresh heartbeat should be alive"
    );
}

#[test]
fn stale_heartbeat_is_dead() {
    let h = PluginHeartbeat::new();
    // Backdate `last` past the timeout. The monotonic clock is always
    // past the system boot, so subtracting a few seconds is representable.
    let stale = Instant::now()
        .checked_sub(HEARTBEAT_TIMEOUT)
        .and_then(|t| t.checked_sub(Duration::from_secs(1)))
        .expect("monotonic clock is well past boot");
    *h.last.lock() = stale;
    assert!(!h.is_alive(HEARTBEAT_TIMEOUT));
}

#[test]
fn handle_heartbeat_updates_counters() {
    let h = PluginHeartbeat::new();
    handle_heartbeat(
        &h,
        &serde_json::json!({"events_total": 3, "invalid_total": 1}),
    );
    assert_eq!(h.events_total.load(Ordering::SeqCst), 3);
    assert_eq!(h.invalid_total.load(Ordering::SeqCst), 1);
}
