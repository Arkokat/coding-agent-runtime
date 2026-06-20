#![allow(clippy::expect_used)]

use agentd::event_bus::Event;
use agentd::handlers::subscriber_registry::SubscriberRegistry;
use chrono::Utc;
use serde_json::json;

#[test]
fn register_unregister() {
    let reg = SubscriberRegistry::new();
    let (id1, _rx1) = reg.register();
    let (id2, _rx2) = reg.register();
    assert_ne!(id1, id2);
    reg.unregister(id1);
    reg.unregister(id1);
    reg.unregister(id2);
}

#[test]
fn broadcast_delivers_to_all_subscribers() {
    let reg = SubscriberRegistry::new();
    let (_id1, mut rx1) = reg.register();
    let (_id2, mut rx2) = reg.register();
    let event = Event {
        kind: "session.created".into(),
        session_id: None,
        payload: json!({}),
        ts: Utc::now(),
    };
    reg.broadcast(&event);
    let r1 = rx1.try_recv().expect("rx1 got event");
    let r2 = rx2.try_recv().expect("rx2 got event");
    assert_eq!(r1.kind, "session.created");
    assert_eq!(r2.kind, "session.created");
}

#[test]
fn broadcast_drops_dead_senders() {
    let reg = SubscriberRegistry::new();
    let (id, rx) = reg.register();
    drop(rx);
    let event = Event {
        kind: "session.killed".into(),
        session_id: None,
        payload: json!({}),
        ts: Utc::now(),
    };
    reg.broadcast(&event);
    let (id2, _rx2) = reg.register();
    assert_ne!(id, id2);
}

#[test]
fn unregister_unknown_id_is_noop() {
    let reg = SubscriberRegistry::new();
    reg.unregister(999);
}
