#![allow(clippy::expect_used)]

use agentd::ipc::framing;
use agentd::plugin_heartbeat::HEARTBEAT_INTERVAL;
use agentd::plugin_uds::bind_and_handshake;
use agentd_testing::test_socket_path;
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

/// The plugin UDS server in this test does a real `connect(2)` to the
/// listener, so it's gated the same way as the other `AF_UNIX` tests in
/// `control_server.rs`. Use `cargo nextest run -p agentd --run-ignored all
/// --test plugin_uds_handshake` to execute on CI.
#[tokio::test]
#[ignore = "needs AF_UNIX support (some local sandboxes block connect)"]
async fn bind_and_handshake_accepts_a_plugin_hello() {
    let sock = test_socket_path("plugin-hello-ok");
    let _ = std::fs::remove_file(&sock);

    let server = tokio::spawn({
        let sock = sock.clone();
        async move { bind_and_handshake(&sock, Duration::from_secs(2)).await }
    });

    // Simulate a plugin client: connect, send plugin.hello, read response.
    let stream = tokio::net::UnixStream::connect(&sock)
        .await
        .expect("connect");
    let (r, mut w) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(r);
    let req = json!({"jsonrpc":"2.0","id":1,"method":"plugin.hello","params":{"name":"opencode","version":"1.0.0","pid":1234}});
    // `framing::write_message` is sync and takes `std::io::Write`. Serialize
    // to a `Vec<u8>` first, then push to the async socket.
    let mut buf: Vec<u8> = Vec::new();
    framing::write_message(&mut buf, &req).expect("serialize");
    w.write_all(&buf).await.expect("write");
    w.flush().await.expect("flush");

    let mut line = String::new();
    reader.read_line(&mut line).await.expect("read");
    let resp: serde_json::Value = serde_json::from_str(line.trim()).expect("parse");
    assert_eq!(resp["result"]["plugin_id"], "opencode");
    assert_eq!(
        resp["result"]["heartbeat_interval_secs"],
        HEARTBEAT_INTERVAL.as_secs()
    );

    let hs = server.await.expect("join").expect("handshake ok");
    assert_eq!(hs.plugin_name, "opencode");
    assert_eq!(hs.version, "1.0.0");
    assert_eq!(hs.pid, Some(1234));

    let _ = std::fs::remove_file(&sock);
}

/// No client connects; the bind must time out and surface `Err`. Gated
/// alongside the connect test so the same `--run-ignored all` invocation
/// runs the whole file.
#[tokio::test]
#[ignore = "needs AF_UNIX support (some local sandboxes block bind)"]
async fn bind_and_handshake_times_out_when_no_client() {
    let sock = test_socket_path("plugin-hello-timeout");
    let _ = std::fs::remove_file(&sock);
    let r = bind_and_handshake(&sock, Duration::from_millis(150)).await;
    let _ = std::fs::remove_file(&sock);
    assert!(r.is_err(), "should time out without a client");
}
