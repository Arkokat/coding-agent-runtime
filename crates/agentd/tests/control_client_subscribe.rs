#![allow(clippy::expect_used)]

use agentd::control_client::ControlClient;
use agentd::event_bus::Event;
use agentd::ipc::framing;
use agentd_testing::test_socket_path;
use serde_json::json;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;

#[tokio::test]
#[ignore = "needs AF_UNIX support (some local sandboxes block bind)"]
async fn subscribe_receives_event_notifications() {
    let sock = test_socket_path("ctrl-subscribe-ok");
    let _ = std::fs::remove_file(&sock);

    // Server: bind, accept one connection, handle subscribe, push one event, then read EOF and exit.
    let listener = UnixListener::bind(&sock).expect("bind");
    std::fs::set_permissions(&sock, std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .expect("chmod");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let (r, mut w) = stream.into_split();
        let mut reader = tokio::io::BufReader::new(r);

        // Read subscribe request.
        let mut line = String::new();
        tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line)
            .await
            .expect("read");
        let req: serde_json::Value = serde_json::from_str(line.trim()).expect("parse");
        assert_eq!(req["method"], "subscribe");

        // Send subscribe response.
        let resp = json!({
            "jsonrpc": "2.0",
            "id": req["id"],
            "result": {"subscription_id": 1, "filter": {}}
        });
        let mut buf = Vec::new();
        framing::write_message(&mut buf, &resp).expect("write");
        tokio::io::AsyncWriteExt::write_all(&mut w, &buf)
            .await
            .expect("write");

        // Push one event notification.
        let event = Event {
            kind: "session.created".into(),
            session_id: None,
            payload: json!({"id": "abc"}),
            ts: chrono::Utc::now(),
        };
        let event_frame = json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": event
        });
        let mut buf = Vec::new();
        framing::write_message(&mut buf, &event_frame).expect("write");
        tokio::io::AsyncWriteExt::write_all(&mut w, &buf)
            .await
            .expect("write");
        w.flush().await.expect("flush");
        // Keep connection open briefly so client can receive.
        tokio::time::sleep(Duration::from_millis(200)).await;
    });

    // Client: open a connection (via subscribe).
    let path = sock.clone();
    let client = ControlClient::connect(&path).await.expect("connect");
    let mut rx = client
        .subscribe(json!({"events": ["session.*"]}))
        .await
        .expect("subscribe");

    // Wait for the event.
    let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("timeout")
        .expect("recv");
    assert_eq!(ev.kind, "session.created");
    assert_eq!(ev.payload.get("id").and_then(|v| v.as_str()), Some("abc"));

    server.abort();
    let _ = std::fs::remove_file(&sock);
}
