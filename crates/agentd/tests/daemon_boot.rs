#![allow(clippy::expect_used)]

use agentd::daemon::{Daemon, DaemonError, acquire_flock};
use agentd::db::Db;
use agentd::event_bus::EventBus;
use agentd::paths::Paths;
use agentd::tmux::MockTmux;
use agentd_testing::test_runtime_dir;

#[allow(unsafe_code)]
fn fresh_paths(label: &str) -> Paths {
    let dir = test_runtime_dir().join(format!("daemon-boot-{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    // Override XDG paths for the duration of the test by setting env vars
    // before resolving Paths.
    // SAFETY: `set_var` is `unsafe` under Rust 2024. Tests are single-threaded
    // at this point and we set XDG paths once before resolving `Paths`.
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", &dir);
        std::env::set_var("XDG_DATA_HOME", &dir);
        std::env::set_var("XDG_STATE_HOME", &dir);
        std::env::set_var("XDG_CONFIG_HOME", &dir);
    }
    Paths::resolve()
}

#[test]
fn second_flock_acquire_fails() {
    let paths = fresh_paths("flock");
    let lock = paths.daemon_lock_path.clone();
    let _g1 = acquire_flock(&lock).expect("first lock");
    let r = acquire_flock(&lock);
    assert!(matches!(r, Err(DaemonError::LockHeld)), "got {r:?}");
}

#[test]
fn daemon_new_resolves_paths_and_does_not_boot() {
    let paths = fresh_paths("new");
    let db = Db::open(&paths.state_db_path).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    let bus = EventBus::default();
    let d = Daemon::new(
        paths,
        db,
        bus,
        Box::new(MockTmux::new()),
        agentd::plugins_manifest::PluginsManifest::default(),
    );
    // Constructing does not bind anything.
    assert!(
        !d.shutdown_handle()
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}
