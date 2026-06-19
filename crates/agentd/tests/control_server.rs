#![allow(clippy::expect_used)]

use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use agentd::ipc::control::ControlServer;
use tempfile::TempDir;

#[tokio::test]
async fn server_accepts_connections_and_invokes_handler() {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("c.sock");
    let server = ControlServer::bind(&sock).expect("bind");
    let addr = server.local_addr().to_path_buf();

    let counter = Arc::new(AtomicUsize::new(0));
    let c2 = counter.clone();
    let join = tokio::spawn(async move {
        server
            .serve(move |_stream| {
                c2.fetch_add(1, Ordering::SeqCst);
            })
            .await;
    });

    let _ = UnixStream::connect(&addr).expect("c1");
    let _ = UnixStream::connect(&addr).expect("c2");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(counter.load(Ordering::SeqCst), 2);

    join.abort();
}

#[tokio::test]
async fn server_sets_socket_permissions_to_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("c.sock");
    let _server = ControlServer::bind(&sock).expect("bind");
    let meta = std::fs::metadata(&sock).expect("meta");
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "socket must be 0600, got {mode:o}");
}

#[tokio::test]
async fn peer_uid_returns_current_uid_for_local_peer() {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("c.sock");
    let _server = ControlServer::bind(&sock).expect("bind");
    let stream = UnixStream::connect(sock).expect("connect");
    let uid = agentd::ipc::control::peer_uid(&stream).expect("uid");
    assert!(uid > 0, "expected nonzero uid, got {uid}");
}
