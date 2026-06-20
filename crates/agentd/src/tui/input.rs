//! TUI key handler. Maps keys to state mutations and `ControlClient` RPCs.

use crate::control_client::ControlClient;
use crate::tui::state::{NewModal, RenameModal, TuiState};
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
            open_new_modal(state, client).await;
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

async fn open_new_modal(state: &mut TuiState, client: &ControlClient) {
    // Query recents from `session.list_active` aggregated by working_dir.
    let recents = match client
        .call(Method::SESSION_LIST_ACTIVE, serde_json::json!({}))
        .await
    {
        Ok(v) => extract_recents(&v),
        Err(_) => Vec::new(),
    };
    state.new_modal = Some(NewModal {
        query: String::new(),
        recents,
    });
}

fn extract_recents(
    value: &serde_json::Value,
) -> Vec<(std::path::PathBuf, chrono::DateTime<chrono::Utc>)> {
    use std::collections::BTreeMap;
    let mut by_dir: BTreeMap<std::path::PathBuf, chrono::DateTime<chrono::Utc>> = BTreeMap::new();
    if let Some(arr) = value.as_array() {
        for s in arr {
            if let (Some(dir), Some(ts)) = (
                s.get("working_dir").and_then(|v| v.as_str()),
                s.get("last_event_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()),
            ) {
                let path = std::path::PathBuf::from(dir);
                let ts_utc = ts.with_timezone(&chrono::Utc);
                by_dir
                    .entry(path)
                    .and_modify(|existing| {
                        if ts_utc > *existing {
                            *existing = ts_utc;
                        }
                    })
                    .or_insert(ts_utc);
            }
        }
    }
    let mut v: Vec<_> = by_dir.into_iter().collect();
    v.sort_by_key(|(_, ts)| std::cmp::Reverse(*ts)); // most recent first
    v.truncate(10);
    v
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
async fn handle_new_modal_key(
    state: &mut TuiState,
    key: KeyEvent,
    _client: &ControlClient,
) -> bool {
    let Some(mut modal) = state.new_modal.take() else {
        return false;
    };
    match key.code {
        KeyCode::Esc => {}
        KeyCode::Enter => {
            // Use the first matching recent, or the typed query.
            let q = modal.query.to_lowercase();
            let target = modal
                .recents
                .iter()
                .find(|(p, _)| q.is_empty() || p.to_string_lossy().to_lowercase().contains(&q))
                .map_or_else(|| std::path::PathBuf::from("."), |(p, _)| p.clone());
            // Stash target for the event loop (Task 7) to pick up via
            // `state.pending_create` and call `session.create` on the daemon.
            state.pending_create = Some(target);
        }
        KeyCode::Backspace => {
            modal.query.pop();
            state.new_modal = Some(modal);
        }
        KeyCode::Char(c) => {
            modal.query.push(c);
            state.new_modal = Some(modal);
        }
        _ => {
            state.new_modal = Some(modal);
        }
    }
    state.dirty = true;
    false
}
