#![allow(clippy::expect_used)]

use agentd::daemon::ensure_daemon_running;
use agentd::paths::Paths;
use agentd_testing::test_runtime_dir;

#[tokio::test]
#[ignore = "needs AF_UNIX support (some local sandboxes block connect)"]
#[allow(unsafe_code)]
async fn ensure_returns_ok_when_socket_exists() {
    let dir = test_runtime_dir().join("lazy-ok");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create");
    // SAFETY: `set_var` is `unsafe` under Rust 2024. Tests are single-threaded
    // at this point and we set XDG paths once before resolving `Paths`.
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", &dir);
        std::env::set_var("XDG_DATA_HOME", &dir);
        std::env::set_var("XDG_STATE_HOME", &dir);
        std::env::set_var("XDG_CONFIG_HOME", &dir);
    }
    let paths = Paths::resolve();

    std::fs::create_dir_all(&paths.runtime_dir).expect("runtime dir");
    let _listener = tokio::net::UnixListener::bind(&paths.control_socket_path).expect("bind");
    let r = ensure_daemon_running(&paths).await;
    assert!(r.is_ok(), "expected Ok, got {r:?}");
}
