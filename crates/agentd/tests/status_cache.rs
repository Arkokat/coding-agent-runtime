#![allow(clippy::expect_used, clippy::single_char_pattern)]

use agentd::db::Db;
use agentd::db::repo::SessionRepo;
use agentd::status::cache::StatusCache;
use agentd_protocol::{AgentType, Session, SessionSource, SessionStatus};
use chrono::Utc;
use uuid::Uuid;

fn fresh_db() -> Db {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let db = Db::open(&dir.path().join("state.db")).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    db
}

fn insert(db: &Db, pane: Option<&str>, status: SessionStatus, task: Option<&str>) -> Uuid {
    let s = Session {
        id: Uuid::now_v7(),
        agent_type: AgentType::Opencode,
        working_dir: "/tmp".into(),
        tmux_session: pane.map(|_| "agentd-x".into()),
        tmux_pane_id: pane.map(str::to_string),
        display_name: "x".into(),
        status,
        current_task: task.map(str::to_string),
        model: Some("claude-sonnet-4-5".into()),
        context_used_tokens: None,
        context_total_tokens: None,
        cost_usd: Some(0.42),
        source: SessionSource::Cli,
        created_at: Utc::now(),
        last_event_at: None,
        finished_at: None,
        metadata: serde_json::json!({}),
    };
    SessionRepo::new(db).insert(&s).expect("insert");
    s.id
}

#[test]
fn format_pane_returns_empty_for_unknown_pane() {
    let cache = StatusCache::new();
    assert_eq!(cache.format_pane("%99"), "");
}

#[test]
fn format_pane_returns_agent_and_task() {
    let db = fresh_db();
    let _ = insert(
        &db,
        Some("%5"),
        SessionStatus::Working,
        Some("editing src/foo.rs"),
    );
    let cache = StatusCache::new();
    cache.rebuild(&db).expect("rebuild");
    let s = cache.format_pane("%5");
    assert!(s.contains("opencode"), "got: {s:?}");
    assert!(s.contains("editing src/foo.rs"), "got: {s:?}");
}

#[test]
fn format_global_counts_agents_by_status() {
    let db = fresh_db();
    let _ = insert(&db, Some("%1"), SessionStatus::Working, None);
    let _ = insert(&db, Some("%2"), SessionStatus::Idle, None);
    let _ = insert(&db, Some("%3"), SessionStatus::WaitingForUser, None);
    let _ = insert(&db, None, SessionStatus::Finished, None); // not counted
    let cache = StatusCache::new();
    cache.rebuild(&db).expect("rebuild");
    let g = cache.format_global();
    assert!(g.contains("3"), "expected count of 3 active, got: {g}");
    assert!(g.contains("working"), "got: {g}");
    assert!(g.contains("idle"), "got: {g}");
    assert!(g.contains("waiting"), "got: {g}");
}

#[test]
fn rebuild_is_fast_under_500ms_for_100_sessions() {
    let db = fresh_db();
    for i in 0..100 {
        let _ = insert(&db, Some(&format!("%{i}")), SessionStatus::Idle, None);
    }
    let cache = StatusCache::new();
    let t0 = std::time::Instant::now();
    cache.rebuild(&db).expect("rebuild");
    let dt = t0.elapsed();
    assert!(
        dt < std::time::Duration::from_millis(500),
        "rebuild too slow: {dt:?}"
    );
}
