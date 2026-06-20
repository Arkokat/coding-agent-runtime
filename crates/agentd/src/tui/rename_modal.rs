//! Rename modal: text input + commit/cancel.

use crate::tui::state::RenameModal;
use crossterm::event::{KeyCode, KeyEvent};

/// Outcome of processing a single key event in the rename modal.
pub enum RenameOutcome {
    /// Keep the modal open with updated state.
    Stay,
    /// Close the modal without taking action.
    Cancel,
    /// Close the modal and commit the new name.
    Commit(String),
}

/// Open a rename modal pre-filled with `initial`. The caller should set
/// `session_id` on the returned modal to the session being renamed.
pub fn open(initial: &str) -> RenameModal {
    RenameModal {
        session_id: uuid::Uuid::nil(), // caller overrides after open
        input: initial.to_string(),
    }
}

/// Apply a key event to the modal. Returns the resulting outcome; the
/// caller is responsible for inspecting the outcome and mutating state
/// accordingly (e.g. dropping the modal on `Cancel`/`Commit`).
pub fn apply_key(modal: &mut RenameModal, key: KeyEvent) -> RenameOutcome {
    match key.code {
        KeyCode::Esc => RenameOutcome::Cancel,
        KeyCode::Enter => {
            let trimmed = modal.input.trim().to_string();
            if trimmed.is_empty() {
                RenameOutcome::Cancel
            } else {
                RenameOutcome::Commit(trimmed)
            }
        }
        KeyCode::Backspace => {
            modal.input.pop();
            RenameOutcome::Stay
        }
        KeyCode::Char(c) => {
            modal.input.push(c);
            RenameOutcome::Stay
        }
        _ => RenameOutcome::Stay,
    }
}
