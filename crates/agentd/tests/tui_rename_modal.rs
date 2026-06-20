#![allow(clippy::expect_used)]

use agentd::tui::rename_modal::{RenameOutcome, apply_key, open};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn open_prefills_with_initial_name() {
    let m = open("old-name");
    assert_eq!(m.input, "old-name");
}

#[test]
fn apply_key_chars_appends() {
    let mut m = open("ab");
    apply_key(&mut m, key(KeyCode::Char('c')));
    assert_eq!(m.input, "abc");
}

#[test]
fn apply_key_backspace_trims() {
    let mut m = open("abc");
    apply_key(&mut m, key(KeyCode::Backspace));
    assert_eq!(m.input, "ab");
}

#[test]
fn apply_key_enter_commits() {
    let mut m = open("hello");
    let outcome = apply_key(&mut m, key(KeyCode::Enter));
    match outcome {
        RenameOutcome::Commit(s) => assert_eq!(s, "hello"),
        _ => panic!("expected Commit"),
    }
}

#[test]
fn apply_key_enter_with_empty_input_cancels() {
    let mut m = open("");
    let outcome = apply_key(&mut m, key(KeyCode::Enter));
    assert!(matches!(outcome, RenameOutcome::Cancel));
}

#[test]
fn apply_key_esc_cancels() {
    let mut m = open("x");
    let outcome = apply_key(&mut m, key(KeyCode::Esc));
    assert!(matches!(outcome, RenameOutcome::Cancel));
}
