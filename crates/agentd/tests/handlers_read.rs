#![allow(clippy::expect_used)]

use agentd::db::Db;
use agentd::db::repo::SessionRepo;
use agentd::handlers::read;
use agentd_protocol::Method;
use agentd_protocol::{AgentType, Session, SessionSource, SessionStatus};
use chrono::Utc;
use uuid::Uuid;

fn fresh_db() -> Db {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let db = Db::open(&dir.path().join("state.db")).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    db
}

fn insert_session(db: &Db) -> Session {
    let s = Session {
        id: Uuid::now_v7(),
        agent_type: AgentType::Opencode,
        working_dir: "/tmp/x".into(),
        tmux_session: None,
        tmux_pane_id: None,
        display_name: "x".into(),
        status: SessionStatus::Starting,
        current_task: None,
        model: None,
        context_used_tokens: None,
        context_total_tokens: None,
        cost_usd: None,
        source: SessionSource::Cli,
        created_at: Utc::now(),
        last_event_at: None,
        finished_at: None,
        metadata: serde_json::json!({}),
    };
    SessionRepo::new(db).insert(&s).expect("insert");
    s
}

#[test]
fn state_capture_returns_sessions_and_plugins() {
    let db = fresh_db();
    let s = insert_session(&db);
    let snap = agentd::state::State::capture(&db).expect("capture");
    assert_eq!(snap.sessions.len(), 1);
    assert_eq!(snap.sessions[0].id, s.id);
    assert!(snap.plugins.is_empty());
}

#[test]
fn dispatch_state_snapshot() {
    let db = fresh_db();
    let _ = insert_session(&db);
    let params = serde_json::json!({});
    let r = read::dispatch(Method::STATE_SNAPSHOT, params, &db).expect("handled");
    assert!(r["sessions"].is_array());
    assert_eq!(r["sessions"].as_array().unwrap().len(), 1);
    assert!(r["plugins"].is_array());
}

#[test]
fn dispatch_session_get() {
    let db = fresh_db();
    let s = insert_session(&db);
    let params = serde_json::json!({"id": s.id.to_string()});
    let r = read::dispatch(Method::SESSION_GET, params, &db).expect("handled");
    assert_eq!(r["id"], s.id.to_string());
}

#[test]
fn dispatch_session_get_missing_returns_none() {
    let db = fresh_db();
    let params = serde_json::json!({"id": Uuid::now_v7().to_string()});
    let r = read::dispatch(Method::SESSION_GET, params, &db);
    assert!(
        r.is_none(),
        "missing session is a router error, not a successful response"
    );
}

#[test]
fn dispatch_session_events_empty() {
    let db = fresh_db();
    let s = insert_session(&db);
    let params = serde_json::json!({"id": s.id.to_string()});
    let r = read::dispatch(Method::SESSION_EVENTS, params, &db).expect("handled");
    assert!(r["events"].is_array());
    assert_eq!(r["events"].as_array().unwrap().len(), 0);
}

#[test]
fn dispatch_daemon_status() {
    let db = fresh_db();
    let r = read::dispatch(Method::DAEMON_STATUS, serde_json::json!({}), &db).expect("handled");
    assert!(r["version"].is_string());
    assert!(r["uptime_secs"].is_number());
}

#[test]
fn dispatch_session_list_active_returns_only_non_finished() {
    let db = fresh_db();
    // Two sessions: one starting, one already finished. The router
    // (read::dispatch) walks SessionRepo::list_non_finished which
    // filters by !is_terminal(); Finished is terminal, so the
    // finished session must be excluded.
    let starting = insert_session(&db);
    let mut finished = insert_session(&db);
    finished.status = SessionStatus::Finished;
    SessionRepo::new(&db)
        .update_status(&finished.id, SessionStatus::Finished, Utc::now())
        .expect("mark finished");

    let r =
        read::dispatch(Method::SESSION_LIST_ACTIVE, serde_json::json!({}), &db).expect("handled");
    let arr = r.as_array().expect("array result");
    assert_eq!(
        arr.len(),
        1,
        "expected only the non-finished session, got {arr:?}"
    );
    let ids: Vec<&str> = arr.iter().filter_map(|s| s["id"].as_str()).collect();
    assert!(
        ids.contains(&starting.id.to_string().as_str()),
        "starting session should be present, got {ids:?}",
    );
    assert!(
        !ids.contains(&finished.id.to_string().as_str()),
        "finished session should be excluded, got {ids:?}",
    );
}

#[test]
fn dispatch_plugin_list_empty() {
    let db = fresh_db();
    let r = read::dispatch(Method::PLUGIN_LIST, serde_json::json!({}), &db).expect("handled");
    assert!(r.is_array());
    assert_eq!(r.as_array().unwrap().len(), 0);
}

#[test]
fn dispatch_unknown_method_returns_none() {
    let db = fresh_db();
    let r = read::dispatch("nonexistent.method", serde_json::json!({}), &db);
    assert!(r.is_none());
}
