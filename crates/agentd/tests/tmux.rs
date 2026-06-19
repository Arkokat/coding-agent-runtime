#![allow(clippy::expect_used)]

use agentd::tmux::{MockTmux, Tmux, validate_pane_id, validate_session_name};

#[test]
fn session_name_accepts_alnum_dot_dash_underscore() {
    assert!(validate_session_name("agentd-019065a1").is_ok());
    assert!(validate_session_name("foo_bar.baz").is_ok());
    assert!(validate_session_name("a").is_ok());
    assert!(validate_session_name("123").is_ok());
}

#[test]
fn session_name_rejects_unsafe_chars() {
    assert!(validate_session_name("foo;rm -rf /").is_err());
    assert!(validate_session_name("foo bar").is_err());
    assert!(validate_session_name("foo$bar").is_err());
    assert!(validate_session_name("").is_err());
    assert!(validate_session_name("../etc/passwd").is_err());
}

#[test]
fn pane_id_accepts_percent_digits() {
    assert!(validate_pane_id("%0").is_ok());
    assert!(validate_pane_id("%123").is_ok());
}

#[test]
fn pane_id_rejects_other_shapes() {
    assert!(validate_pane_id("5").is_err());
    assert!(validate_pane_id("%").is_err());
    assert!(validate_pane_id("%abc").is_err());
    assert!(validate_pane_id("%-1").is_err());
}

#[tokio::test]
async fn mock_tmux_new_session_records_pane() {
    let tmux = MockTmux::new();
    let pane = tmux.new_session("agentd-x", "/tmp").await.expect("new");
    assert!(pane.starts_with('%'));
    assert!(tmux.has_session("agentd-x").await);
    let panes = tmux.list_panes().await.expect("list");
    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].session, "agentd-x");
}

#[tokio::test]
async fn mock_tmux_kill_removes_session() {
    let tmux = MockTmux::new();
    let _ = tmux.new_session("a", "/tmp").await.expect("new");
    tmux.kill_session("a").await.expect("kill");
    assert!(!tmux.has_session("a").await);
}
