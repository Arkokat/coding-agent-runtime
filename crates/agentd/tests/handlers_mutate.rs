#![allow(clippy::expect_used)]

use agentd::db::Db;
use agentd::db::repo::SessionRepo;
use agentd::handlers::mutate;
use agentd::tmux::{MockTmux, Tmux};
use agentd_protocol::{AgentType, Method, Session, SessionSource, SessionStatus};
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
        working_dir: "/tmp/p".into(),
        tmux_session: None,
        tmux_pane_id: None,
        display_name: "p".into(),
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
fn session_create_inserts_new_row() {
    mutate::reset_shutdown_for_tests();
    let db = fresh_db();
    let tmux = MockTmux::new();
    let r = mutate::dispatch(
        Method::SESSION_CREATE,
        serde_json::json!({
            "agent_type": "opencode",
            "working_dir": "/tmp/q",
            "name": "q",
        }),
        &db,
        &tmux,
    );
    let v = r.into_value().expect("ok");
    assert_eq!(v["working_dir"], "/tmp/q");
    assert_eq!(v["display_name"], "q");
    assert_eq!(v["status"], "starting");
    assert_eq!(v["source"], "cli");
    assert_eq!(SessionRepo::new(&db).list().expect("list").len(), 1);
}

#[test]
fn session_create_rejects_unknown_agent_type() {
    mutate::reset_shutdown_for_tests();
    let db = fresh_db();
    let tmux = MockTmux::new();
    let r = mutate::dispatch(
        Method::SESSION_CREATE,
        serde_json::json!({"agent_type": "mystery", "working_dir": "/tmp/x"}),
        &db,
        &tmux,
    );
    let err = r.into_err().expect("err");
    assert_eq!(err.code(), -32602);
}

#[test]
fn session_rename_updates_display_name() {
    mutate::reset_shutdown_for_tests();
    let db = fresh_db();
    let tmux = MockTmux::new();
    let id = insert(&db);
    let r = mutate::dispatch(
        Method::SESSION_RENAME,
        serde_json::json!({"id": id.to_string(), "name": "renamed"}),
        &db,
        &tmux,
    );
    let v = r.into_value().expect("ok");
    assert_eq!(v["display_name"], "renamed");
    let got = SessionRepo::new(&db)
        .get(&id)
        .expect("get")
        .expect("present");
    assert_eq!(got.display_name, "renamed");
}

#[test]
fn session_rename_missing_returns_session_not_found() {
    mutate::reset_shutdown_for_tests();
    let db = fresh_db();
    let tmux = MockTmux::new();
    let r = mutate::dispatch(
        Method::SESSION_RENAME,
        serde_json::json!({"id": Uuid::now_v7().to_string(), "name": "x"}),
        &db,
        &tmux,
    );
    let err = r.into_err().expect("err");
    assert_eq!(err.code(), -32001);
}

#[test]
fn session_dismiss_error_clears_errored_status() {
    mutate::reset_shutdown_for_tests();
    let db = fresh_db();
    let tmux = MockTmux::new();
    let id = insert(&db);
    SessionRepo::new(&db)
        .update_status(&id, SessionStatus::Errored, chrono::Utc::now())
        .expect("e");
    let r = mutate::dispatch(
        Method::SESSION_DISMISS_ERROR,
        serde_json::json!({"id": id.to_string()}),
        &db,
        &tmux,
    );
    let v = r.into_value().expect("ok");
    assert_eq!(v["status"], "idle");
}

#[test]
fn session_dismiss_error_only_works_on_errored() {
    mutate::reset_shutdown_for_tests();
    let db = fresh_db();
    let tmux = MockTmux::new();
    let id = insert(&db);
    let r = mutate::dispatch(
        Method::SESSION_DISMISS_ERROR,
        serde_json::json!({"id": id.to_string()}),
        &db,
        &tmux,
    );
    let err = r.into_err().expect("err");
    assert_eq!(err.code(), -32602, "must reject non-errored status");
}

#[test]
fn session_kill_removes_tmux_and_marks_finished() {
    let db = fresh_db();
    let tmux = MockTmux::new();
    let id = insert_started_session(&db, "killable");

    agentd::db::repo::SessionRepo::new(&db)
        .update_tmux(&id, Some("killable"), Some("%1"))
        .expect("update_tmux");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _ = rt.block_on(tmux.new_session("killable", "/tmp"));

    let r = mutate::dispatch(
        Method::SESSION_KILL,
        serde_json::json!({"id": id.to_string()}),
        &db,
        &tmux,
    );
    let v: serde_json::Value = serde_json::to_value(&r).unwrap();
    assert_eq!(v["status"], "finished", "got {v}");

    let r2 = mutate::dispatch(
        Method::SESSION_KILL,
        serde_json::json!({"id": id.to_string()}),
        &db,
        &tmux,
    );
    let err = r2.into_err().expect("err");
    assert_eq!(err.code(), -32001);
}

#[test]
fn session_kill_unknown_session_returns_not_found() {
    let db = fresh_db();
    let tmux = MockTmux::new();
    let r = mutate::dispatch(
        Method::SESSION_KILL,
        serde_json::json!({"id": Uuid::now_v7().to_string()}),
        &db,
        &tmux,
    );
    let err = r.into_err().expect("err");
    assert_eq!(err.code(), -32001);
}

#[test]
fn session_jump_succeeds_for_existing_tmux_session() {
    let db = fresh_db();
    let tmux = MockTmux::new();
    let id = insert_started_session(&db, "jumpy");
    agentd::db::repo::SessionRepo::new(&db)
        .update_tmux(&id, Some("jumpy"), Some("%1"))
        .expect("update_tmux");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _ = rt.block_on(tmux.new_session("jumpy", "/tmp"));

    let r = mutate::dispatch(
        Method::SESSION_JUMP,
        serde_json::json!({"id": id.to_string()}),
        &db,
        &tmux,
    );
    let v: serde_json::Value = serde_json::to_value(&r).unwrap();
    assert_eq!(v["ok"], true);
}

#[test]
fn session_jump_unknown_session_returns_not_found() {
    let db = fresh_db();
    let tmux = MockTmux::new();
    let r = mutate::dispatch(
        Method::SESSION_JUMP,
        serde_json::json!({"id": Uuid::now_v7().to_string()}),
        &db,
        &tmux,
    );
    let err = r.into_err().expect("err");
    assert_eq!(err.code(), -32001);
}

// Helper: insert a `starting` session row with the given display name.
fn insert_started_session(db: &agentd::db::Db, name: &str) -> uuid::Uuid {
    let s = Session {
        id: Uuid::now_v7(),
        agent_type: AgentType::Opencode,
        working_dir: "/tmp/x".into(),
        tmux_session: None,
        tmux_pane_id: None,
        display_name: name.into(),
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
fn daemon_shutdown_sets_flag() {
    mutate::reset_shutdown_for_tests();
    let db = fresh_db();
    let tmux = MockTmux::new();
    let r = mutate::dispatch(Method::DAEMON_SHUTDOWN, serde_json::json!({}), &db, &tmux);
    let v = r.into_value().expect("ok");
    assert_eq!(v["ok"], true);
    assert!(mutate::shutdown_requested());
    mutate::reset_shutdown_for_tests();
}
