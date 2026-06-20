#![allow(clippy::expect_used)]
#![allow(unused_imports)]

use agentd::event_bus::Event;
use agentd::tui::{NewModal, RenameModal, StatusCounters, TuiState};
use agentd_protocol::{AgentType, Session, SessionSource, SessionStatus};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

fn sample_session(id: Uuid, status: SessionStatus) -> Session {
    Session {
        id,
        agent_type: AgentType::Opencode,
        working_dir: "/tmp/x".into(),
        tmux_session: Some("x".into()),
        tmux_pane_id: Some("%1".into()),
        display_name: format!("s-{id}"),
        status,
        current_task: Some("editing".into()),
        model: None,
        context_used_tokens: Some(42000),
        context_total_tokens: Some(200_000),
        cost_usd: Some(0.12),
        source: SessionSource::Cli,
        created_at: Utc::now(),
        last_event_at: None,
        finished_at: None,
        metadata: json!({}),
    }
}

#[test]
fn from_snapshot_parses_sessions_and_plugins() {
    let s1 = sample_session(Uuid::now_v7(), SessionStatus::Working);
    let s2 = sample_session(Uuid::now_v7(), SessionStatus::Idle);
    let snap = json!({
        "sessions": [serde_json::to_value(&s1).unwrap(), serde_json::to_value(&s2).unwrap()],
        "plugins": [],
    });
    let state = TuiState::from_snapshot(&snap);
    assert_eq!(state.sessions.len(), 2);
    assert_eq!(state.counters.working, 1);
    assert_eq!(state.counters.idle, 1);
    assert!(state.dirty);
}

#[test]
fn from_snapshot_empty_snap() {
    let state = TuiState::from_snapshot(&json!({}));
    assert_eq!(state.sessions.len(), 0);
    assert_eq!(state.plugins.len(), 0);
    assert_eq!(state.counters, StatusCounters::default());
}

#[test]
fn apply_event_session_created_inserts_and_flashes() {
    let mut state = TuiState::new();
    let id = Uuid::now_v7();
    let s = sample_session(id, SessionStatus::Working);
    let event = Event {
        kind: "session.created".into(),
        session_id: Some(id),
        payload: serde_json::to_value(&s).unwrap(),
        ts: Utc::now(),
    };
    state.apply_event(&event);
    assert_eq!(state.sessions.len(), 1);
    assert_eq!(state.sessions[0].id, id);
    assert!(state.flash_until.contains_key(&id));
    assert_eq!(state.counters.working, 1);
    assert!(state.dirty);
}

#[test]
fn apply_event_session_renamed_updates_name() {
    let mut state = TuiState::new();
    let id = Uuid::now_v7();
    let mut s = sample_session(id, SessionStatus::Working);
    state.sessions.push(s.clone());
    s.display_name = "renamed".into();
    let event = Event {
        kind: "session.renamed".into(),
        session_id: Some(id),
        payload: json!({"display_name": "renamed"}),
        ts: Utc::now(),
    };
    state.apply_event(&event);
    assert_eq!(state.sessions[0].display_name, "renamed");
}

#[test]
fn apply_event_session_killed_removes() {
    let mut state = TuiState::new();
    let id = Uuid::now_v7();
    state
        .sessions
        .push(sample_session(id, SessionStatus::Working));
    let event = Event {
        kind: "session.killed".into(),
        session_id: Some(id),
        payload: json!({}),
        ts: Utc::now(),
    };
    state.apply_event(&event);
    assert!(state.sessions.is_empty());
    assert!(!state.flash_until.contains_key(&id));
}

#[test]
fn apply_event_session_status_changed_updates_status() {
    let mut state = TuiState::new();
    let id = Uuid::now_v7();
    state
        .sessions
        .push(sample_session(id, SessionStatus::Working));
    let event = Event {
        kind: "session.status_changed".into(),
        session_id: Some(id),
        payload: json!({"status": "errored"}),
        ts: Utc::now(),
    };
    state.apply_event(&event);
    assert_eq!(state.sessions[0].status, SessionStatus::Errored);
    assert_eq!(state.counters.errored, 1);
    assert_eq!(state.counters.working, 0);
}

#[test]
fn apply_event_session_task_changed_updates_task() {
    let mut state = TuiState::new();
    let id = Uuid::now_v7();
    state
        .sessions
        .push(sample_session(id, SessionStatus::Working));
    let event = Event {
        kind: "session.task_changed".into(),
        session_id: Some(id),
        payload: json!({"task": "writing tests"}),
        ts: Utc::now(),
    };
    state.apply_event(&event);
    assert_eq!(
        state.sessions[0].current_task.as_deref(),
        Some("writing tests")
    );
}

#[test]
fn tick_flash_clears_expired() {
    use std::time::Duration;
    let mut state = TuiState::new();
    let id = Uuid::now_v7();
    let past = std::time::Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("now is at least 1s past epoch");
    state.flash_until.insert(id, past);
    state.dirty = false;
    state.tick_flash(std::time::Instant::now());
    assert!(!state.flash_until.contains_key(&id));
    assert!(state.dirty);
}

#[test]
fn selected_session_returns_indexed() {
    let mut state = TuiState::new();
    state
        .sessions
        .push(sample_session(Uuid::now_v7(), SessionStatus::Idle));
    state
        .sessions
        .push(sample_session(Uuid::now_v7(), SessionStatus::Idle));
    state.selected = 1;
    assert_eq!(state.selected_session().unwrap().id, state.sessions[1].id);
}

#[test]
fn selected_session_empty_when_no_sessions() {
    let state = TuiState::new();
    assert!(state.selected_session().is_none());
}
