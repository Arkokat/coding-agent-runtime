#![allow(clippy::expect_used)]

use agentd::db::Db;
use agentd::db::repo::SessionRepo;
use agentd::handlers::plugin_handlers;
use agentd_protocol::{Method, SessionSource, SessionStatus};
use uuid::Uuid;

fn fresh_db() -> Db {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let db = Db::open(&dir.path().join("state.db")).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    db
}

fn insert(db: &Db) -> Uuid {
    let s = agentd_protocol::Session {
        id: Uuid::now_v7(),
        agent_type: agentd_protocol::AgentType::Opencode,
        working_dir: "/tmp/q".into(),
        tmux_session: None,
        tmux_pane_id: None,
        display_name: "q".into(),
        status: SessionStatus::Starting,
        current_task: None,
        model: None,
        context_used_tokens: None,
        context_total_tokens: None,
        cost_usd: None,
        source: SessionSource::Cli,
        created_at: chrono::Utc::now(),
        last_event_at: None,
        finished_at: None,
        metadata: serde_json::json!({}),
    };
    SessionRepo::new(db).insert(&s).expect("insert");
    s.id
}

#[test]
fn plugin_hello_returns_heartbeat_interval() {
    let db = fresh_db();
    let r = plugin_handlers::dispatch(
        Method::PLUGIN_HELLO,
        serde_json::json!({"name": "opencode", "version": "1.0.0", "pid": 1234, "binary_path": "/usr/bin/agentd-plugin-opencode"}),
        &db,
        "opencode",
    );
    let v = r.expect("ok");
    assert_eq!(v["ok"], true);
    assert_eq!(v["heartbeat_interval_secs"], 5);
}

#[test]
fn session_discover_inserts_row_and_returns_id() {
    let db = fresh_db();
    let r = plugin_handlers::dispatch(
        Method::SESSION_DISCOVER,
        serde_json::json!({
            "tmux_session": "agentd-x",
            "tmux_pane_id": "%7",
            "working_dir": "/tmp/y",
        }),
        &db,
        "opencode",
    );
    let v = r.expect("ok");
    assert_eq!(v["ok"], true);
    let id_str = v["session_id"].as_str().expect("id str");
    let id = Uuid::parse_str(id_str).expect("parse");
    let s = SessionRepo::new(&db)
        .get(&id)
        .expect("get")
        .expect("present");
    assert_eq!(s.source, SessionSource::Discovered);
    assert_eq!(s.tmux_pane_id.as_deref(), Some("%7"));
}

#[test]
fn session_report_event_writes_to_event_log() {
    let db = fresh_db();
    let id = insert(&db);
    let r = plugin_handlers::dispatch(
        Method::SESSION_REPORT_EVENT,
        serde_json::json!({
            "session_id": id.to_string(),
            "type": "session.started",
            "payload": {"x": 1},
            "ts": "2026-06-19T00:00:00Z",
        }),
        &db,
        "opencode",
    );
    let v = r.expect("ok");
    assert_eq!(v["accepted"], true);
    let events = agentd::db::repo::EventRepo::new(&db)
        .list_for_session(&id)
        .expect("list");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "session.started");
}

#[test]
fn session_report_event_missing_session_errors() {
    let db = fresh_db();
    let r = plugin_handlers::dispatch(
        Method::SESSION_REPORT_EVENT,
        serde_json::json!({
            "session_id": Uuid::now_v7().to_string(),
            "type": "session.started",
            "payload": {},
            "ts": "2026-06-19T00:00:00Z",
        }),
        &db,
        "opencode",
    );
    let err = r.expect_err("err");
    assert_eq!(err.code(), -32001);
}

#[test]
fn session_report_event_returns_event_rowid_not_session_id() {
    let db = fresh_db();
    let id = insert(&db);
    let r = plugin_handlers::dispatch(
        Method::SESSION_REPORT_EVENT,
        serde_json::json!({
            "session_id": id.to_string(),
            "type": "session.started",
            "payload": {"x": 1},
            "ts": "2026-06-19T00:00:00Z",
        }),
        &db,
        "opencode",
    );
    let v = r.expect("ok");
    let event_id = v["event_id"].as_str().expect("event_id str");
    let event_id_uuid = Uuid::parse_str(event_id);
    assert!(
        event_id_uuid.is_err(),
        "event_id should be the event rowid (integer), not the session uuid; got {event_id}"
    );
    let event_id_int: i64 = event_id
        .parse()
        .expect("event_id should be a numeric rowid string");
    assert!(event_id_int > 0, "event_id should be a positive rowid");
}

#[test]
fn session_report_event_uses_agent_ts_for_last_event_at() {
    let db = fresh_db();
    let id = insert(&db);
    let agent_ts = "2025-01-15T12:34:56Z";
    let r = plugin_handlers::dispatch(
        Method::SESSION_REPORT_EVENT,
        serde_json::json!({
            "session_id": id.to_string(),
            "type": "session.task_changed",
            "payload": {"task": "writing code"},
            "ts": agent_ts,
        }),
        &db,
        "opencode",
    );
    let _ = r.expect("ok");
    let s = SessionRepo::new(&db)
        .get(&id)
        .expect("get")
        .expect("present");
    let last = s
        .last_event_at
        .expect("last_event_at should be set")
        .to_rfc3339();
    assert_eq!(last, "2025-01-15T12:34:56+00:00");
}

#[test]
fn plugin_heartbeat_returns_counts() {
    let db = fresh_db();
    let r = plugin_handlers::dispatch(
        Method::PLUGIN_HEARTBEAT,
        serde_json::json!({}),
        &db,
        "opencode",
    );
    let v = r.expect("ok");
    assert_eq!(v["ok"], true);
    assert!(v["events_total"].is_number());
}

#[test]
fn plugin_bye_marks_disconnect() {
    let db = fresh_db();
    let r = plugin_handlers::dispatch(Method::PLUGIN_BYE, serde_json::json!({}), &db, "opencode");
    let v = r.expect("ok");
    assert_eq!(v["ok"], true);
}
