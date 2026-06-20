#![allow(clippy::expect_used)]

use std::sync::Arc;

use agentd::daemon::{Daemon, DaemonError, acquire_flock, restart_respawn, tombstone_gc};
use agentd::db::Db;
use agentd::db::repo::SessionRepo;
use agentd::event_bus::EventBus;
use agentd::paths::Paths;
use agentd::plugin_spawner::{MockPluginSpawner, PluginSpawner};
use agentd::plugin_supervisor::PluginSupervisor;
use agentd::plugins_manifest::PluginsManifest;
use agentd::tmux::MockTmux;
use agentd_protocol::{AgentType, Session, SessionSource, SessionStatus};
use agentd_testing::test_runtime_dir;
use chrono::{Duration, Utc};
use parking_lot::Mutex;
use std::path::PathBuf;
use uuid::Uuid;

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
    let calls: Arc<Mutex<Vec<(String, PathBuf, PathBuf)>>> = Arc::new(Mutex::new(Vec::new()));
    let spawner: Arc<dyn PluginSpawner> = Arc::new(MockPluginSpawner::new(Arc::clone(&calls)));
    let d = Daemon::new(
        paths,
        db,
        bus,
        Arc::new(MockTmux::new()),
        agentd::plugins_manifest::PluginsManifest::default(),
        spawner,
    );
    // Constructing does not bind anything.
    assert!(
        !d.shutdown_handle()
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

fn insert_finished(db: &Db, days_ago: i64) -> Uuid {
    let id = Uuid::now_v7();
    let mut s = Session {
        id,
        agent_type: AgentType::Opencode,
        working_dir: "/tmp/x".into(),
        tmux_session: None,
        tmux_pane_id: None,
        display_name: "x".into(),
        status: SessionStatus::Finished,
        current_task: None,
        model: None,
        context_used_tokens: None,
        context_total_tokens: None,
        cost_usd: None,
        source: SessionSource::Cli,
        created_at: Utc::now() - Duration::days(60),
        last_event_at: None,
        finished_at: Some(Utc::now() - Duration::days(days_ago)),
        metadata: serde_json::json!({}),
    };
    // `created_at` is set by the repo from `Utc::now()`; for this test we
    // only care about `finished_at`, so we insert with a row update.
    s.status = SessionStatus::Starting;
    SessionRepo::new(db).insert(&s).expect("insert");
    // Force finished + finished_at via raw SQL (the public repo doesn't
    // accept an arbitrary `finished_at`).
    db.conn()
        .execute(
            "UPDATE sessions SET status = 'finished', finished_at = ?1 WHERE id = ?2",
            rusqlite::params![
                (Utc::now() - Duration::days(days_ago)).to_rfc3339(),
                id.to_string()
            ],
        )
        .expect("update");
    id
}

#[test]
fn tombstone_gc_removes_old_finished_sessions() {
    let paths = fresh_paths("gc");
    let db = Db::open(&paths.state_db_path).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    let old = insert_finished(&db, 45);
    let recent = insert_finished(&db, 5);

    let deleted = tombstone_gc(&db).expect("gc");
    assert_eq!(deleted, 1, "exactly one old session should be deleted");

    let remaining: Vec<String> = db
        .conn()
        .prepare("SELECT id FROM sessions")
        .expect("prepare")
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .filter_map(Result::ok)
        .collect();
    assert!(!remaining.contains(&old.to_string()));
    assert!(remaining.contains(&recent.to_string()));
}

#[test]
fn tombstone_gc_no_op_when_nothing_old() {
    let paths = fresh_paths("gc-empty");
    let db = Db::open(&paths.state_db_path).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    let _ = insert_finished(&db, 1);
    let deleted = tombstone_gc(&db).expect("gc");
    assert_eq!(deleted, 0);
}

#[ignore = "needs AF_UNIX support (some local sandboxes block bind)"]
#[tokio::test]
async fn daemon_run_executes_boot_sequence_and_binds_control_uds() {
    let paths = fresh_paths("boot-bind");
    let db = Db::open(&paths.state_db_path).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    let bus = EventBus::default();
    let toml = r#"
[[plugins]]
name = "opencode"
binary = "/bin/true"
autostart = true
"#;
    let manifest: PluginsManifest = toml::from_str(toml).expect("parse manifest");
    let calls: Arc<Mutex<Vec<(String, PathBuf, PathBuf)>>> = Arc::new(Mutex::new(Vec::new()));
    let spawner: Arc<dyn PluginSpawner> = Arc::new(MockPluginSpawner::new(Arc::clone(&calls)));

    // Construct a daemon that has the spawner wired through the supervisor.
    let d = Daemon::new(
        paths.clone(),
        db,
        bus,
        Arc::new(MockTmux::new()),
        manifest,
        spawner,
    );
    let shutdown = d.shutdown_handle();
    let socket_path = paths.control_socket_path.clone();

    // The daemon holds !Send state (rusqlite::Connection is !Sync, so
    // `PluginSupervisor` is !Sync, so `&self.supervisor` is !Send). Use a
    // LocalSet so the daemon can run concurrently with this test's polling
    // task without needing a Send future.
    let local = tokio::task::LocalSet::new();
    let daemon_handle = local.spawn_local(d.run());

    // Poll for the control UDS to appear, yielding to the daemon task
    // between checks via the LocalSet.
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
    assert!(bound, "control UDS should be bound");

    shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
    let r = local.run_until(daemon_handle).await.expect("join");
    r.expect("daemon run ok");
    assert!(!calls.lock().is_empty(), "plugin should have been spawned");
}

#[tokio::test]
async fn restart_respawn_spawns_plugins_for_non_finished_sessions() {
    let paths = fresh_paths("restart");
    let db = Db::open(&paths.state_db_path).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");

    // Insert a non-finished session with `metadata.plugin = "opencode"`.
    let id = uuid::Uuid::now_v7();
    db.conn()
        .execute(
            "INSERT INTO sessions (id, agent_type, working_dir, display_name, status, source, created_at, metadata)
             VALUES (?1, 'opencode', '/tmp/x', 'x', 'starting', 'cli', ?2, ?3)",
            rusqlite::params![
                id.to_string(),
                chrono::Utc::now().to_rfc3339(),
                serde_json::json!({"plugin": "opencode"}).to_string()
            ],
        )
        .expect("insert");

    let toml = r#"
[[plugins]]
name = "opencode"
binary = "/usr/bin/true"
autostart = false
"#;
    let manifest: PluginsManifest = toml::from_str(toml).expect("parse");
    let bus = EventBus::default();
    let calls: Arc<parking_lot::Mutex<Vec<(String, PathBuf, PathBuf)>>> =
        Arc::new(parking_lot::Mutex::new(Vec::new()));
    let spawner: Arc<dyn PluginSpawner> = Arc::new(MockPluginSpawner::new(Arc::clone(&calls)));
    let mut sup = PluginSupervisor::new(bus, &db, manifest, spawner);
    sup.set_paths(paths.clone());

    let n = restart_respawn(&db, &sup).await.expect("restart");
    assert_eq!(n, 1, "opencode should have been spawned once");

    let recorded = calls.lock().clone();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].0, "opencode");
}
