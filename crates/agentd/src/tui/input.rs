//! TUI key handler. Maps keys to state mutations and `ControlClient` RPCs.

use crate::control_client::ControlClient;
use crate::tui::new_modal;
use crate::tui::state::{RenameModal, TuiState};
use agentd_protocol::Method;
use crossterm::event::{KeyCode, KeyEvent};
use std::time::Instant;

/// Handle a key event. Returns `true` if the TUI should quit.
pub async fn handle_key(state: &mut TuiState, key: KeyEvent, client: &ControlClient) -> bool {
    // Modal-first: if a modal is open, route keys there.
    if state.rename_modal.is_some() {
        return handle_rename_modal_key(state, key, client).await;
    }
    if state.new_modal.is_some() {
        return handle_new_modal_key(state, key, client).await;
    }
    if state.show_help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?' | 'q')) {
            state.show_help = false;
            if key.code == KeyCode::Char('q') {
                return true;
            }
        }
        return false;
    }

    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => return true,
        KeyCode::Char('c' | 'C') => {
            let modal = new_modal::open(client).await;
            state.new_modal = Some(modal);
        }
        KeyCode::Char('r' | 'R') => {
            open_rename_modal(state);
        }
        KeyCode::Char('?') => {
            state.show_help = true;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if !state.sessions.is_empty() {
                state.selected = (state.selected + 1).min(state.sessions.len() - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Char('g') => state.selected = 0,
        KeyCode::Char('G') => {
            if !state.sessions.is_empty() {
                state.selected = state.sessions.len() - 1;
            }
        }
        KeyCode::Enter => {
            jump_to_selected(state, client).await;
        }
        KeyCode::Char('x' | 'X') => {
            kill_selected(state, client).await;
        }
        _ => {}
    }
    state.dirty = true;
    false
}

fn open_rename_modal(state: &mut TuiState) {
    if let Some(s) = state.selected_session() {
        state.rename_modal = Some(RenameModal {
            session_id: s.id,
            input: s.display_name.clone(),
        });
    }
}

async fn jump_to_selected(state: &mut TuiState, client: &ControlClient) {
    let Some(session) = state.selected_session() else {
        return;
    };
    let id = session.id;
    let r = client
        .call(
            Method::SESSION_JUMP,
            serde_json::json!({"id": id.to_string()}),
        )
        .await;
    match r {
        Ok(_) => state.status_message = Some(("Jumped to session".into(), Instant::now())),
        Err(e) => state.status_message = Some((format!("Jump failed: {e}"), Instant::now())),
    }
}

async fn kill_selected(state: &mut TuiState, client: &ControlClient) {
    let Some(session) = state.selected_session() else {
        return;
    };
    let id = session.id;
    let r = client
        .call(
            Method::SESSION_KILL,
            serde_json::json!({"id": id.to_string()}),
        )
        .await;
    match r {
        Ok(_) => {
            state.status_message = Some((
                format!("Killed session {}", session.display_name),
                Instant::now(),
            ));
        }
        Err(e) => state.status_message = Some((format!("Kill failed: {e}"), Instant::now())),
    }
}

async fn handle_rename_modal_key(
    state: &mut TuiState,
    key: KeyEvent,
    client: &ControlClient,
) -> bool {
    let Some(mut modal) = state.rename_modal.take() else {
        return false;
    };
    match key.code {
        KeyCode::Esc => {}
        KeyCode::Enter => {
            let id = modal.session_id;
            let new_name = modal.input.trim().to_string();
            if !new_name.is_empty() {
                let _ = client
                    .call(
                        Method::SESSION_RENAME,
                        serde_json::json!({"id": id.to_string(), "name": new_name}),
                    )
                    .await;
            }
        }
        KeyCode::Backspace => {
            modal.input.pop();
            state.rename_modal = Some(modal);
        }
        KeyCode::Char(c) => {
            modal.input.push(c);
            state.rename_modal = Some(modal);
        }
        _ => {
            state.rename_modal = Some(modal);
        }
    }
    state.dirty = true;
    false
}

#[allow(clippy::unused_async)] // kept `async` for symmetry with handle_rename_modal_key
async fn handle_new_modal_key(state: &mut TuiState, key: KeyEvent, client: &ControlClient) -> bool {
    let Some(mut modal) = state.new_modal.take() else {
        return false;
    };
    let outcome = new_modal::apply_key(&mut modal, key);
    match outcome {
        new_modal::NewModalOutcome::Stay => {
            state.new_modal = Some(modal);
        }
        new_modal::NewModalOutcome::Cancel => {
            state.new_modal = None;
        }
        new_modal::NewModalOutcome::Commit(path) => {
            state.new_modal = None;
            let _ = client
                .call(
                    Method::SESSION_CREATE,
                    serde_json::json!({
                        "agent_type": "opencode",
                        "working_dir": path.to_string_lossy(),
                        "name": path.file_name().and_then(|n| n.to_str()).unwrap_or("session"),
                    }),
                )
                .await;
            state.status_message = Some((
                format!("Creating session in {}", path.display()),
                Instant::now(),
            ));
        }
    }
    state.dirty = true;
    false
}
