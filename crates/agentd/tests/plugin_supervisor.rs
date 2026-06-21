#![allow(clippy::expect_used)]

use parking_lot::Mutex;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

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
#[ignore = "needs AF_UNIX support (some local sandboxes block bind)"]
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

#[tokio::test]
#[ignore = "needs AF_UNIX support (some local sandboxes block bind)"]
async fn bind_and_serve_dispatches_plugin_calls() {
    let (paths, db, bus) = fresh("dispatch");
    let calls: Arc<Mutex<Vec<(String, PathBuf, PathBuf)>>> = Arc::new(Mutex::new(Vec::new()));
    let spawner: Arc<dyn PluginSpawner> = Arc::new(MockPluginSpawner::new(Arc::clone(&calls)));
    let sup = PluginSupervisor::new(bus, &db, two_plugin_manifest(), spawner);

    let bound = sup
        .bind_and_serve("test_plugin", &paths)
        .expect("bind_and_serve");
    assert_eq!(bound, paths.plugin_socket_path("test_plugin"));

    let mut client = UnixStream::connect(&bound).await.expect("connect");

    // 1. plugin.hello
    send_request(
        &mut client,
        1,
        "plugin.hello",
        json!({"name": "test_plugin", "version": "0.1.0"}),
    )
    .await;
    let resp = read_response(&mut client).await;
    assert_eq!(resp["result"]["plugin_id"], "test_plugin");
    assert_eq!(resp["result"]["heartbeat_interval_secs"], 5);

    // 2. plugin.heartbeat
    send_request(&mut client, 2, "plugin.heartbeat", json!({})).await;
    let resp = read_response(&mut client).await;
    assert_eq!(resp["result"]["ok"], true);

    // 3. session.discover
    send_request(
        &mut client,
        3,
        "session.discover",
        json!({
            "tmux_session": "%1",
            "tmux_pane_id": "%0",
            "working_dir": "/tmp/proj",
        }),
    )
    .await;
    let resp = read_response(&mut client).await;
    assert_eq!(resp["result"]["ok"], true);
    let session_id = resp["result"]["session_id"]
        .as_str()
        .expect("session_id string");
    assert!(
        uuid::Uuid::parse_str(session_id).is_ok(),
        "session_id must be a uuid, got {session_id}"
    );

    sup.shutdown().await;
    let _ = std::fs::remove_file(&bound);
}

#[tokio::test]
#[ignore = "needs AF_UNIX support (some local sandboxes block bind)"]
async fn bind_and_serve_is_idempotent_for_same_name() {
    let (paths, db, bus) = fresh("idempotent");
    let calls: Arc<Mutex<Vec<(String, PathBuf, PathBuf)>>> = Arc::new(Mutex::new(Vec::new()));
    let spawner: Arc<dyn PluginSpawner> = Arc::new(MockPluginSpawner::new(Arc::clone(&calls)));
    let sup = PluginSupervisor::new(bus, &db, two_plugin_manifest(), spawner);

    let first = sup
        .bind_and_serve("test_plugin", &paths)
        .expect("first bind");
    let second = sup
        .bind_and_serve("test_plugin", &paths)
        .expect("second bind");
    assert_eq!(first, second);

    // Server still functional: one client can connect after the
    // duplicate bind.
    let mut client = UnixStream::connect(&first).await.expect("connect");
    send_request(
        &mut client,
        1,
        "plugin.hello",
        json!({"name": "test_plugin"}),
    )
    .await;
    let resp = read_response(&mut client).await;
    assert_eq!(resp["result"]["plugin_id"], "test_plugin");

    sup.shutdown().await;
    let _ = std::fs::remove_file(&first);
}

async fn send_request(client: &mut UnixStream, id: u64, method: &str, params: Value) {
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let mut bytes = serde_json::to_vec(&req).expect("encode");
    bytes.push(b'\n');
    client.write_all(&bytes).await.expect("write");
}

async fn read_response(client: &mut UnixStream) -> Value {
    let mut reader = tokio::io::BufReader::new(client);
    let mut line = String::new();
    let n = reader.read_line(&mut line).await.expect("read");
    assert!(n > 0, "expected response, got EOF");
    serde_json::from_str(line.trim()).expect("parse response")
}
