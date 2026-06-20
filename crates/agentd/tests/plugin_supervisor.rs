#![allow(clippy::expect_used)]

use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

use agentd::db::Db;
use agentd::event_bus::EventBus;
use agentd::paths::Paths;
use agentd::plugin_spawner::{MockPluginSpawner, PluginSpawner};
use agentd::plugin_supervisor::PluginSupervisor;
use agentd_testing::test_runtime_dir;

#[allow(unsafe_code)]
fn fresh(label: &str) -> (Paths, Db, EventBus) {
    let dir = test_runtime_dir().join(format!("plugin-supervisor-real-{label}"));
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
    let db = Db::open(&paths.state_db_path).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    (paths, db, EventBus::default())
}

fn two_plugin_manifest() -> agentd::plugins_manifest::PluginsManifest {
    let toml = r#"
[[plugins]]
name = "opencode"
binary = "/usr/bin/true"
autostart = true

[[plugins]]
name = "claude-code"
binary = "/usr/bin/true"
autostart = true
"#;
    toml::from_str(toml).expect("parse manifest")
}

#[tokio::test]
async fn autostart_spawns_every_plugin_in_manifest() {
    let (paths, db, bus) = fresh("spawns");
    let calls: Arc<Mutex<Vec<(String, PathBuf, PathBuf)>>> = Arc::new(Mutex::new(Vec::new()));
    let spawner: Arc<dyn PluginSpawner> = Arc::new(MockPluginSpawner::new(Arc::clone(&calls)));
    let sup = PluginSupervisor::new(bus, &db, two_plugin_manifest(), spawner);
    let n = sup.autostart(&paths).await.expect("autostart");
    assert_eq!(n, 2, "both plugins should have been spawned");

    let recorded = calls.lock().clone();
    let names: Vec<String> = recorded.iter().map(|c| c.0.clone()).collect();
    assert!(names.contains(&"opencode".to_string()));
    assert!(names.contains(&"claude-code".to_string()));

    let beats = sup.heartbeats_snapshot();
    assert!(beats.contains_key("opencode"));
    assert!(beats.contains_key("claude-code"));

    sup.shutdown().await;
}

#[tokio::test]
async fn autostart_skips_non_autostart_plugins() {
    let (paths, db, bus) = fresh("skips");
    let toml = r#"
[[plugins]]
name = "opencode"
binary = "/usr/bin/true"
autostart = false
"#;
    let manifest: agentd::plugins_manifest::PluginsManifest = toml::from_str(toml).expect("parse");
    let calls: Arc<Mutex<Vec<(String, PathBuf, PathBuf)>>> = Arc::new(Mutex::new(Vec::new()));
    let spawner: Arc<dyn PluginSpawner> = Arc::new(MockPluginSpawner::new(Arc::clone(&calls)));
    let sup = PluginSupervisor::new(bus, &db, manifest, spawner);
    let n = sup.autostart(&paths).await.expect("autostart");
    assert_eq!(n, 0);
    assert!(calls.lock().is_empty());
}
