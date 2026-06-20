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
