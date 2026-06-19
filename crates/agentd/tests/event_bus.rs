#![allow(clippy::expect_used)]

use agentd::event_bus::{Event, EventBus};
use serde_json::json;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

#[tokio::test]
async fn subscriber_receives_emitted_event() {
    let bus = EventBus::new(16);
    let mut rx = bus.subscribe();
    bus.emit(Event {
        kind: "session.status_changed".into(),
        session_id: Some(Uuid::now_v7()),
        payload: json!({"status": "working"}),
        ts: chrono::Utc::now(),
    });
    let got = timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("not timed out")
        .expect("not lagged");
    assert_eq!(got.kind, "session.status_changed");
    assert_eq!(got.payload["status"], "working");
}

#[tokio::test]
async fn multiple_subscribers_all_receive() {
    let bus = EventBus::new(16);
    let mut rx1 = bus.subscribe();
    let mut rx2 = bus.subscribe();
    bus.emit(Event {
        kind: "session.started".into(),
        session_id: None,
        payload: json!({}),
        ts: chrono::Utc::now(),
    });
    let e1 = timeout(Duration::from_millis(100), rx1.recv()).await.expect("t1").expect("r1");
    let e2 = timeout(Duration::from_millis(100), rx2.recv()).await.expect("t2").expect("r2");
    assert_eq!(e1.kind, "session.started");
    assert_eq!(e2.kind, "session.started");
}

#[tokio::test]
async fn emit_with_no_subscribers_is_a_no_op() {
    let bus = EventBus::new(16);
    let n = bus.emit(Event {
        kind: "session.finished".into(),
        session_id: None,
        payload: json!({}),
        ts: chrono::Utc::now(),
    });
    assert_eq!(n, 0);
}
