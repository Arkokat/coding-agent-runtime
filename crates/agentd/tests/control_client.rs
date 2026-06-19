#![allow(clippy::expect_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use agentd::control_client::ControlClient;
use agentd::ipc::control::ControlServer;
use agentd::ipc::framing;
use serde_json::json;
use std::os::unix::net::UnixStream;
use tempfile::TempDir;

#[tokio::test]
async fn client_sends_request_and_reads_response() {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("c.sock");
    let server = ControlServer::bind(&sock).expect("bind");

    let counter = Arc::new(AtomicUsize::new(0));
    let c2 = counter.clone();
    let join = tokio::spawn(async move {
        server
            .serve(move |mut stream| {
                // Read one request, write one canned response.
                let _ = c2.fetch_add(1, Ordering::SeqCst);
                let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                let _ = framing::read_message(&mut reader);
                let resp = json!({"jsonrpc":"2.0","id":1,"result":{"ok":true}});
                let _ = framing::write_message(&mut stream, &resp);
            })
            .await;
    });

    let client = ControlClient::connect(&sock).await.expect("connect");
    let v = client.call("ping", json!({})).await.expect("call");
    assert_eq!(v["ok"], true);
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    let _ = UnixStream::connect(&sock); // sanity
    join.abort();
}
