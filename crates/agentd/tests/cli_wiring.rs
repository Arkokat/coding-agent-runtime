#![allow(clippy::expect_used, unsafe_code)]

#[ignore = "needs AF_UNIX support (some local sandboxes block it)"]
#[tokio::test]
async fn foreground_daemon_starts_and_shuts_down() {
    // Use a unique XDG_RUNTIME_DIR.
    let dir = agentd_testing::test_runtime_dir().join("cli-fg");
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
    let paths = agentd::paths::Paths::resolve();
    let db = agentd::db::Db::open(&paths.state_db_path).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    let bus = agentd::event_bus::EventBus::default();
    let d = agentd::daemon::Daemon::new(
        paths.clone(),
        db,
        bus,
        Box::new(agentd::tmux::MockTmux::new()),
        agentd::plugins_manifest::PluginsManifest::default(),
    );
    let shutdown = d.shutdown_handle();
    let socket_path = paths.control_socket_path.clone();

    // Daemon::run is !Send (rusqlite::Connection is !Send), so we drive it
    // on a LocalSet the same way `daemon_boot.rs::daemon_run_executes_*`
    // does.
    let local = tokio::task::LocalSet::new();
    let daemon_handle = local.spawn_local(d.run());

    let bound = local
        .run_until(async {
            for _ in 0..40 {
                if socket_path.exists() {
                    return true;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            false
        })
        .await;
    assert!(bound, "control UDS should appear");

    shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
    let r = local.run_until(daemon_handle).await.expect("join");
    r.expect("daemon run");
    assert!(
        !paths.control_socket_path.exists(),
        "control UDS should be cleaned up on shutdown"
    );
}

#[ignore = "needs AF_UNIX support (some local sandboxes block it)"]
#[test]
fn detach_writes_pid_and_unblocks_parent() {
    use std::process::Command;
    let dir = agentd_testing::test_runtime_dir().join("cli-detach");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create");
    // Build the agentd binary if not already.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let _ = Command::new(&cargo)
        .args(["build", "-p", "agentd"])
        .status()
        .expect("build");
    // `CARGO_BIN_EXE_<name>` is set by cargo for integration tests.
    let exe = std::env::var("CARGO_BIN_EXE_agentd")
        .map(std::path::PathBuf::from)
        .expect("CARGO_BIN_EXE_agentd must be set by cargo for this test");

    // Paths::resolve() appends "agentd" to XDG_RUNTIME_DIR (XDG convention),
    // so the PID file lives at <XDG_RUNTIME_DIR>/agentd/daemon.pid and the
    // control UDS lives at <XDG_RUNTIME_DIR>/agentd/control.sock.
    let pid_path = dir.join("agentd").join("daemon.pid");
    let control_sock = dir.join("agentd").join("control.sock");
    let mut child = Command::new(&exe)
        .args(["daemon", "start", "--detach"])
        .env("XDG_RUNTIME_DIR", &dir)
        .env("XDG_DATA_HOME", &dir)
        .env("XDG_STATE_HOME", &dir)
        .env("XDG_CONFIG_HOME", &dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn detach");
    let _ = child.wait();
    // PID file should appear within 2s.
    for _ in 0..40 {
        if pid_path.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        pid_path.exists(),
        "PID file should exist at {}",
        pid_path.display()
    );
    let pid_str = std::fs::read_to_string(&pid_path).expect("read pid");
    let pid: i32 = pid_str.trim().parse().expect("parse pid");
    assert!(pid > 0);
    // Cleanup: send SIGTERM to the daemon; the daemon's signal handler
    // flips the shutdown atomic and the run loop exits. The double-fork
    // grandchild's PID is in the file, so this targets the right process.
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }
    // Wait for the daemon to release the control UDS (up to 5s).
    for _ in 0..100 {
        if !control_sock.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
