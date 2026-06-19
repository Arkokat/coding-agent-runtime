#![allow(clippy::expect_used)]

use std::time::Duration;

use agentd::event_bus::EventBus;
use agentd::paths::Paths;
use agentd::plugin_supervisor::PluginSupervisor;
use agentd::plugins_manifest::PluginsManifest;
use tempfile::TempDir;

fn fresh_paths() -> (TempDir, Paths) {
    let root = TempDir::new().expect("tempdir");
    let paths = Paths::resolve_with(root.path());
    paths.ensure().expect("ensure");
    (root, paths)
}

#[tokio::test]
async fn supervisor_can_be_constructed_with_empty_manifest() {
    let (_root, paths) = fresh_paths();
    let bus = EventBus::default();
    let db = agentd::db::Db::open(&paths.state_db_path).expect("db");
    agentd::db::migrations::run(&db).expect("migrate");
    let m = PluginsManifest::default();
    let sup = PluginSupervisor::new(bus, &db, m);
    assert_eq!(sup.connected_count(), 0);
}

#[tokio::test]
async fn autostart_skips_when_no_autostart_entries() {
    let (_root, paths) = fresh_paths();
    let bus = EventBus::default();
    let db = agentd::db::Db::open(&paths.state_db_path).expect("db");
    agentd::db::migrations::run(&db).expect("migrate");
    let m = PluginsManifest::default();
    let sup = PluginSupervisor::new(bus, &db, m);
    // No real Tmux needed: autostart is a no-op when manifest is empty.
    let started = sup.autostart(&paths).await.expect("autostart");
    assert_eq!(started, 0);
}

#[tokio::test]
async fn event_bus_round_trip_through_supervisor() {
    let (_root, paths) = fresh_paths();
    let bus = EventBus::default();
    let db = agentd::db::Db::open(&paths.state_db_path).expect("db");
    agentd::db::migrations::run(&db).expect("migrate");
    let m = PluginsManifest::default();
    let sup = PluginSupervisor::new(bus, &db, m);
    let mut rx = sup.subscribe();
    sup.bus().emit(agentd::event_bus::Event {
        kind: "session.status_changed".into(),
        session_id: None,
        payload: serde_json::json!({"status": "working"}),
        ts: chrono::Utc::now(),
    });
    let got = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("not timed out")
        .expect("not lagged");
    assert_eq!(got.kind, "session.status_changed");
}
