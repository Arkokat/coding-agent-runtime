#![allow(clippy::expect_used)]
#![allow(non_snake_case)]

use agentd::control_client::ControlClient;
use agentd::tui::input;
use agentd::tui::state::TuiState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[tokio::test]
async fn j_moves_selection_down() {
    let mut state = TuiState::new();
    state.sessions.push(agentd_testing::sample_session(
        Uuid::now_v7(),
        "a",
        agentd_protocol::SessionStatus::Idle,
    ));
    state.sessions.push(agentd_testing::sample_session(
        Uuid::now_v7(),
        "b",
        agentd_protocol::SessionStatus::Idle,
    ));
    state.selected = 0;
    let client = open_stub_client().await;
    assert!(!input::handle_key(&mut state, key(KeyCode::Char('j')), &client).await);
    assert_eq!(state.selected, 1);
}

#[tokio::test]
async fn k_moves_selection_up() {
    let mut state = TuiState::new();
    state.sessions.push(agentd_testing::sample_session(
        Uuid::now_v7(),
        "a",
        agentd_protocol::SessionStatus::Idle,
    ));
    state.sessions.push(agentd_testing::sample_session(
        Uuid::now_v7(),
        "b",
        agentd_protocol::SessionStatus::Idle,
    ));
    state.selected = 1;
    let client = open_stub_client().await;
    assert!(!input::handle_key(&mut state, key(KeyCode::Char('k')), &client).await);
    assert_eq!(state.selected, 0);
}

#[tokio::test]
async fn g_and_G_jump_to_bounds() {
    let mut state = TuiState::new();
    for i in 0..5 {
        state.sessions.push(agentd_testing::sample_session(
            Uuid::now_v7(),
            &format!("s{i}"),
            agentd_protocol::SessionStatus::Idle,
        ));
    }
    state.selected = 2;
    let client = open_stub_client().await;
    input::handle_key(&mut state, key(KeyCode::Char('g')), &client).await;
    assert_eq!(state.selected, 0);
    input::handle_key(&mut state, key(KeyCode::Char('G')), &client).await;
    assert_eq!(state.selected, 4);
}

#[tokio::test]
async fn q_quits() {
    let mut state = TuiState::new();
    let client = open_stub_client().await;
    assert!(input::handle_key(&mut state, key(KeyCode::Char('q')), &client).await);
}

#[tokio::test]
async fn question_mark_toggles_help() {
    let mut state = TuiState::new();
    let client = open_stub_client().await;
    assert!(!state.show_help);
    input::handle_key(&mut state, key(KeyCode::Char('?')), &client).await;
    assert!(state.show_help);
    input::handle_key(&mut state, key(KeyCode::Char('?')), &client).await;
    assert!(!state.show_help);
}

#[tokio::test]
async fn esc_closes_modal() {
    let mut state = TuiState::new();
    state.show_help = true;
    let client = open_stub_client().await;
    input::handle_key(&mut state, key(KeyCode::Esc), &client).await;
    assert!(!state.show_help);
}

#[tokio::test]
async fn r_opens_rename_modal() {
    let mut state = TuiState::new();
    let id = Uuid::now_v7();
    state.sessions.push(agentd_testing::sample_session(
        id,
        "old",
        agentd_protocol::SessionStatus::Idle,
    ));
    let client = open_stub_client().await;
    input::handle_key(&mut state, key(KeyCode::Char('r')), &client).await;
    assert!(state.rename_modal.is_some());
    assert_eq!(state.rename_modal.as_ref().unwrap().input, "old");
}

#[tokio::test]
async fn c_opens_new_modal() {
    let mut state = TuiState::new();
    let client = open_stub_client().await;
    input::handle_key(&mut state, key(KeyCode::Char('c')), &client).await;
    assert!(state.new_modal.is_some());
}

#[test]
fn up_down_keys_move_selection() {
    let mut state = TuiState::new();
    state.sessions.push(agentd_testing::sample_session(
        Uuid::now_v7(),
        "a",
        agentd_protocol::SessionStatus::Idle,
    ));
    state.sessions.push(agentd_testing::sample_session(
        Uuid::now_v7(),
        "b",
        agentd_protocol::SessionStatus::Idle,
    ));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let client = open_stub_client().await;
        input::handle_key(&mut state, key(KeyCode::Down), &client).await;
        assert_eq!(state.selected, 1);
        input::handle_key(&mut state, key(KeyCode::Up), &client).await;
        assert_eq!(state.selected, 0);
    });
}

// Helper: open a ControlClient that never sends real RPCs. We point at a
// socket path that does not exist, so any actual `call` fails fast with
// Io(NotFound); the input handler treats that as "no recents" / no-op.
async fn open_stub_client() -> ControlClient {
    let sock = format!("/tmp/agentd-tui-test-{}.sock", Uuid::now_v7());
    ControlClient::connect(std::path::Path::new(&sock))
        .await
        .expect("connect")
}
