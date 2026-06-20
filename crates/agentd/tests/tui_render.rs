#![allow(clippy::expect_used)]

use agentd::tui::{NewModal, RenameModal, TuiState};
use agentd_protocol::{AgentType, Session, SessionSource, SessionStatus};
use chrono::Utc;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::json;
use std::path::PathBuf;
use uuid::Uuid;

fn sample_session(id: Uuid, name: &str, status: SessionStatus) -> Session {
    Session {
        id,
        agent_type: AgentType::Opencode,
        working_dir: "/tmp/proj".into(),
        tmux_session: Some(name.to_string()),
        tmux_pane_id: Some("%1".into()),
        display_name: name.to_string(),
        status,
        current_task: Some("editing main.rs".into()),
        model: Some("claude-sonnet-4".into()),
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

fn render_to_string(state: &TuiState, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| agentd::tui::render::render(f, state))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let mut s = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            s.push_str(buffer[(x, y)].symbol());
        }
        s.push('\n');
    }
    s
}

#[test]
fn render_empty_state_at_80x24() {
    let state = TuiState::new();
    let s = render_to_string(&state, 80, 24);
    insta::assert_snapshot!(s);
}

#[test]
fn render_one_working_session_at_80x24() {
    let mut state = TuiState::new();
    state.sessions.push(sample_session(
        Uuid::now_v7(),
        "alpha",
        SessionStatus::Working,
    ));
    let s = render_to_string(&state, 80, 24);
    insta::assert_snapshot!(s);
}

#[test]
fn render_mixed_sessions_at_80x24() {
    let mut state = TuiState::new();
    state.sessions.push(sample_session(
        Uuid::now_v7(),
        "alpha",
        SessionStatus::Working,
    ));
    state
        .sessions
        .push(sample_session(Uuid::now_v7(), "beta", SessionStatus::Idle));
    state.sessions.push(sample_session(
        Uuid::now_v7(),
        "gamma",
        SessionStatus::WaitingForUser,
    ));
    state.sessions.push(sample_session(
        Uuid::now_v7(),
        "delta",
        SessionStatus::Errored,
    ));
    let s = render_to_string(&state, 80, 24);
    insta::assert_snapshot!(s);
}

#[test]
fn render_at_200x50() {
    let mut state = TuiState::new();
    for i in 0..10 {
        let name = format!("sess-{i}");
        let status = match i % 4 {
            0 => SessionStatus::Working,
            1 => SessionStatus::Idle,
            2 => SessionStatus::WaitingForUser,
            _ => SessionStatus::Errored,
        };
        state
            .sessions
            .push(sample_session(Uuid::now_v7(), &name, status));
    }
    let s = render_to_string(&state, 200, 50);
    insta::assert_snapshot!(s);
}

#[test]
fn render_help_modal_at_80x24() {
    let mut state = TuiState::new();
    state.show_help = true;
    let s = render_to_string(&state, 80, 24);
    insta::assert_snapshot!(s);
}

#[test]
fn render_rename_modal_at_80x24() {
    let mut state = TuiState::new();
    state.sessions.push(sample_session(
        Uuid::now_v7(),
        "alpha",
        SessionStatus::Working,
    ));
    state.rename_modal = Some(RenameModal {
        session_id: state.sessions[0].id,
        input: "renamed".into(),
    });
    let s = render_to_string(&state, 80, 24);
    insta::assert_snapshot!(s);
}

#[test]
fn render_new_modal_at_80x24() {
    let mut state = TuiState::new();
    state.new_modal = Some(NewModal {
        query: "age".into(),
        recents: vec![
            (PathBuf::from("/home/u/proj/agentd"), Utc::now()),
            (PathBuf::from("/home/u/proj/blog"), Utc::now()),
        ],
    });
    let s = render_to_string(&state, 80, 24);
    insta::assert_snapshot!(s);
}
