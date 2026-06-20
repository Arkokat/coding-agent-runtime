#![allow(clippy::expect_used)]

use agentd::db::Db;
use agentd::event_bus::Event;
use agentd::handlers::mutate;
use agentd::handlers::plugin_handlers;
use agentd::tmux::MockTmux;
use agentd_protocol::Method;
use chrono::Utc;
use serde_json::json;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn fresh_db() -> Db {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&dir.path().join("state.db")).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    db
}

fn last_event(captured: &Mutex<Vec<Event>>) -> Event {
    let g = captured.lock().unwrap();
    g.last().cloned().expect("at least one event")
}

fn insert_started_session(db: &Db, name: &str) -> Uuid {
    let id = Uuid::now_v7();
    db.conn()
        .execute(
            "INSERT INTO sessions
         (id, agent_type, working_dir, status, source, created_at, display_name, metadata)
         VALUES (?1, 'opencode', '/tmp/x', 'starting', 'cli', ?2, ?3, ?4)",
            rusqlite::params![
                id.to_string(),
                Utc::now().to_rfc3339(),
                name,
                json!({"plugin": "opencode"}).to_string(),
            ],
        )
        .expect("insert");
    id
}

#[test]
fn session_create_emits_session_created() {
    let db = fresh_db();
    let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let bus = agentd::event_bus::EventBus::new(16);
    let bus_clone = bus.clone();
    let captured_clone = Arc::clone(&captured);
    let mut rx = bus.subscribe();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            // Drain events into captured.
            tokio::spawn(async move {
                while let Ok(e) = rx.recv().await {
                    captured_clone.lock().unwrap().push(e);
                }
            });
            let r = mutate::dispatch(
                Method::SESSION_CREATE,
                json!({"agent_type":"opencode","working_dir":"/tmp/y","name":"new"}),
                &db,
                &MockTmux::new(),
                &bus_clone,
            );
            let _ = serde_json::to_value(&r).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        });
    let ev = last_event(&captured);
    assert_eq!(ev.kind, "session.created");
    assert!(ev.session_id.is_some());
}

#[test]
fn session_rename_emits_session_renamed() {
    let db = fresh_db();
    let id = insert_started_session(&db, "old");
    let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let bus = agentd::event_bus::EventBus::new(16);
    let mut rx = bus.subscribe();
    let captured_clone = Arc::clone(&captured);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::spawn(async move {
                while let Ok(e) = rx.recv().await {
                    captured_clone.lock().unwrap().push(e);
                }
            });
            let r = mutate::dispatch(
                Method::SESSION_RENAME,
                json!({"id": id.to_string(), "name": "new-name"}),
                &db,
                &MockTmux::new(),
                &bus,
            );
            let _ = serde_json::to_value(&r).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        });
    let ev = last_event(&captured);
    assert_eq!(ev.kind, "session.renamed");
    assert_eq!(ev.session_id, Some(id));
    assert_eq!(
        ev.payload.get("display_name").and_then(|v| v.as_str()),
        Some("new-name")
    );
}

#[test]
fn session_kill_emits_session_killed() {
    let db = fresh_db();
    let id = insert_started_session(&db, "killme");
    let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let bus = agentd::event_bus::EventBus::new(16);
    let mut rx = bus.subscribe();
    let captured_clone = Arc::clone(&captured);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::spawn(async move {
                while let Ok(e) = rx.recv().await {
                    captured_clone.lock().unwrap().push(e);
                }
            });
            let r = mutate::dispatch(
                Method::SESSION_KILL,
                json!({"id": id.to_string()}),
                &db,
                &MockTmux::new(),
                &bus,
            );
            let _ = serde_json::to_value(&r).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        });
    let ev = last_event(&captured);
    assert_eq!(ev.kind, "session.killed");
    assert_eq!(ev.session_id, Some(id));
}

#[test]
fn plugin_hello_emits_plugin_connected() {
    let db = fresh_db();
    let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let bus = agentd::event_bus::EventBus::new(16);
    let mut rx = bus.subscribe();
    let captured_clone = Arc::clone(&captured);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::spawn(async move {
                while let Ok(e) = rx.recv().await {
                    captured_clone.lock().unwrap().push(e);
                }
            });
            let r = plugin_handlers::dispatch(
                Method::PLUGIN_HELLO,
                json!({"name":"opencode","version":"1.0","pid":1234}),
                &db,
                "opencode",
                &bus,
            );
            let _ = serde_json::to_value(&r).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        });
    let ev = last_event(&captured);
    assert_eq!(ev.kind, "plugin.connected");
    assert_eq!(
        ev.payload.get("name").and_then(|v| v.as_str()),
        Some("opencode")
    );
}

#[test]
fn report_event_status_changed_emits() {
    let db = fresh_db();
    let id = insert_started_session(&db, "reportme");
    let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let bus = agentd::event_bus::EventBus::new(16);
    let mut rx = bus.subscribe();
    let captured_clone = Arc::clone(&captured);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::spawn(async move {
                while let Ok(e) = rx.recv().await {
                    captured_clone.lock().unwrap().push(e);
                }
            });
            let r = plugin_handlers::dispatch(
                Method::SESSION_REPORT_EVENT,
                json!({
                    "session_id": id.to_string(),
                    "type": "session.status_changed",
                    "payload": {"status": "working"},
                    "ts": "2026-06-20T00:00:00Z",
                }),
                &db,
                "opencode",
                &bus,
            );
            let _ = serde_json::to_value(&r).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        });
    let ev = last_event(&captured);
    assert_eq!(ev.kind, "session.status_changed");
    assert_eq!(ev.session_id, Some(id));
    assert_eq!(
        ev.payload.get("status").and_then(|v| v.as_str()),
        Some("working")
    );
}
