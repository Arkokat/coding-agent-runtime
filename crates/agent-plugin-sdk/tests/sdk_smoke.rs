#![allow(clippy::expect_used)]

use agentd_plugin_sdk::{Backend, Event, MockBackend};
use std::path::PathBuf;

#[tokio::test]
async fn mock_backend_emits_scripted_events_in_order() {
    let mut be = MockBackend::new(vec![
        Event { kind: "session.started".into(), payload: serde_json::json!({}) },
        Event { kind: "session.status_changed".into(), payload: serde_json::json!({"status": "working"}) },
    ]);
    let e1 = be.next_event().await.expect("e1");
    assert_eq!(e1.kind, "session.started");
    let e2 = be.next_event().await.expect("e2");
    assert_eq!(e2.kind, "session.status_changed");
    assert!(be.next_event().await.is_none());
}

#[test]
fn sdk_re_exports_protocol_version() {
    assert_eq!(agentd_plugin_sdk::SDK_PROTOCOL_VERSION, agentd_protocol::PROTOCOL_VERSION);
}

#[test]
fn agentd_client_connect_returns_error_for_missing_socket() {
    let bad = PathBuf::from("/tmp/agentd-sdk-no-such-socket.sock");
    let r = futures::executor::block_on(agentd_plugin_sdk::AgentdClient::connect(&bad));
    assert!(r.is_err());
}
