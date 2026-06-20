//! New-session modal: recents query + filter + commit/cancel.

use crate::control_client::ControlClient;
use crate::tui::state::NewModal;
use agentd_protocol::Method;
use crossterm::event::{KeyCode, KeyEvent};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Outcome of processing a single key event in the new-session modal.
pub enum NewModalOutcome {
    /// Keep the modal open with updated state.
    Stay,
    /// Close the modal without taking action.
    Cancel,
    /// Close the modal and create a session in the given working directory.
    Commit(PathBuf),
}

/// Open a new modal with recents loaded from the daemon.
///
/// Calls `session.list_active` on `client` and aggregates the result by
/// `working_dir` (keeping the most recent `last_event_at` per dir), then
/// returns an empty-query modal pre-populated with those recents. If the
/// RPC fails for any reason, the modal opens with an empty recents list.
pub async fn open(client: &ControlClient) -> NewModal {
    let recents = match client
        .call(Method::SESSION_LIST_ACTIVE, serde_json::json!({}))
        .await
    {
        Ok(v) => extract_recents(&v),
        Err(_) => Vec::new(),
    };
    NewModal {
        query: String::new(),
        recents,
    }
}

/// Apply a key event to the modal. Returns the resulting outcome; the
/// caller is responsible for inspecting the outcome and mutating state
/// accordingly (e.g. dropping the modal on `Cancel`/`Commit`).
pub fn apply_key(modal: &mut NewModal, key: KeyEvent) -> NewModalOutcome {
    match key.code {
        KeyCode::Esc => NewModalOutcome::Cancel,
        KeyCode::Enter => {
            let target = filtered(modal)
                .into_iter()
                .next()
                .map_or_else(|| PathBuf::from("."), |(p, _)| p);
            NewModalOutcome::Commit(target)
        }
        KeyCode::Backspace => {
            modal.query.pop();
            NewModalOutcome::Stay
        }
        KeyCode::Char(c) => {
            modal.query.push(c);
            NewModalOutcome::Stay
        }
        _ => NewModalOutcome::Stay,
    }
}

/// Filter the recents list by the modal's query (case-insensitive substring).
pub fn filtered(modal: &NewModal) -> Vec<(PathBuf, chrono::DateTime<chrono::Utc>)> {
    let q = modal.query.to_lowercase();
    modal
        .recents
        .iter()
        .filter(|(p, _)| q.is_empty() || p.to_string_lossy().to_lowercase().contains(&q))
        .cloned()
        .collect()
}

fn extract_recents(value: &serde_json::Value) -> Vec<(PathBuf, chrono::DateTime<chrono::Utc>)> {
    let mut by_dir: BTreeMap<PathBuf, chrono::DateTime<chrono::Utc>> = BTreeMap::new();
    if let Some(arr) = value.as_array() {
        for s in arr {
            if let (Some(dir), Some(ts)) = (
                s.get("working_dir").and_then(|v| v.as_str()),
                s.get("last_event_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()),
            ) {
                let path = PathBuf::from(dir);
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
    v.sort_by_key(|(_, ts)| std::cmp::Reverse(*ts));
    v.truncate(10);
    v
}
