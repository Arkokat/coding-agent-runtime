#![allow(clippy::expect_used, unused_mut)]

use agentd::db::{Db, repo::{EventRepo, PluginRepo, SessionRepo}};
use agentd_protocol::{AgentType, Session, SessionSource, SessionStatus};
use chrono::Utc;
use uuid::Uuid;

fn fresh_db() -> Db {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let db = Db::open(&dir.path().join("state.db")).expect("open");
    agentd::db::migrations::run(&db).expect("migrate");
    db
}

fn sample_session() -> Session {
    Session {
        id: Uuid::now_v7(),
        agent_type: AgentType::Opencode,
        working_dir: "/tmp/proj".into(),
        tmux_session: None,
        tmux_pane_id: None,
        display_name: "proj".into(),
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
    }
}

#[test]
fn session_insert_and_get_roundtrip() {
    let db = fresh_db();
    let s = sample_session();
    SessionRepo::new(&db).insert(&s).expect("insert");
    let got = SessionRepo::new(&db).get(&s.id).expect("get").expect("present");
    assert_eq!(got.id, s.id);
    assert_eq!(got.working_dir, s.working_dir);
    assert_eq!(got.status, SessionStatus::Starting);
    assert_eq!(got.source, SessionSource::Cli);
}

#[test]
fn session_list_returns_all_in_insertion_order_by_created_at() {
    let db = fresh_db();
    let repo = SessionRepo::new(&db);
    for _ in 0..3 {
        let s = sample_session();
        repo.insert(&s).expect("insert");
    }
    let all = repo.list().expect("list");
    assert_eq!(all.len(), 3);
}

#[test]
fn session_list_non_finished_excludes_finished() {
    let db = fresh_db();
    let repo = SessionRepo::new(&db);
    let a = sample_session();
    let mut b = sample_session();
    repo.insert(&a).expect("insert a");
    repo.insert(&b).expect("insert b");
    repo.mark_finished(&b.id).expect("finish b");
    let active = repo.list_non_finished().expect("list");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, a.id);
}

#[test]
fn session_update_status_writes_last_event_at() {
    let db = fresh_db();
    let repo = SessionRepo::new(&db);
    let s = sample_session();
    repo.insert(&s).expect("insert");
    repo.update_status(&s.id, SessionStatus::Working).expect("update");
    let got = repo.get(&s.id).expect("get").expect("present");
    assert_eq!(got.status, SessionStatus::Working);
    assert!(got.last_event_at.is_some());
}

#[test]
fn session_update_tmux_uses_unique_index() {
    let db = fresh_db();
    let repo = SessionRepo::new(&db);
    let a = sample_session();
    let b = sample_session();
    repo.insert(&a).expect("insert a");
    repo.insert(&b).expect("insert b");
    repo.update_tmux(&a.id, Some("s1"), Some("%1")).expect("a tmux");
    repo.update_tmux(&b.id, Some("s2"), Some("%1")).expect("b tmux");
    let got_a = repo.get(&a.id).expect("get a").expect("present");
    let got_b = repo.get(&b.id).expect("get b").expect("present");
    assert_eq!(got_a.tmux_pane_id.as_deref(), Some("%1"));
    assert_eq!(got_b.tmux_pane_id.as_deref(), Some("%1"));
    assert_ne!(got_a.tmux_session, got_b.tmux_session);
}

#[test]
fn event_insert_links_to_session() {
    let db = fresh_db();
    let srepo = SessionRepo::new(&db);
    let erepo = EventRepo::new(&db);
    let s = sample_session();
    srepo.insert(&s).expect("insert");
    let id = erepo
        .insert(&s.id, "session.started", &serde_json::json!({"x": 1}))
        .expect("insert event");
    assert!(id > 0);
    let events = erepo.list_for_session(&s.id).expect("list");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "session.started");
}

#[test]
fn event_cascade_deletes_with_session() {
    let db = fresh_db();
    let srepo = SessionRepo::new(&db);
    let erepo = EventRepo::new(&db);
    let s = sample_session();
    srepo.insert(&s).expect("insert");
    erepo.insert(&s.id, "session.started", &serde_json::json!({})).expect("e");
    db.conn().execute("DELETE FROM sessions WHERE id = ?1", [s.id.to_string()]).expect("delete");
    assert!(erepo.list_for_session(&s.id).expect("list").is_empty());
}

#[test]
fn plugin_upsert_and_list() {
    let db = fresh_db();
    let prepo = PluginRepo::new(&db);
    prepo.upsert("opencode", "agentd-plugin-opencode", "opencode.sock", true).expect("upsert");
    prepo.upsert("claude-code", "agentd-plugin-claude-code", "claude-code.sock", true).expect("upsert");
    let list = prepo.list().expect("list");
    assert_eq!(list.len(), 2);
    let oc = list.iter().find(|p| p.name == "opencode").expect("opencode");
    assert_eq!(oc.binary, "agentd-plugin-opencode");
    assert!(oc.autostart);
}
