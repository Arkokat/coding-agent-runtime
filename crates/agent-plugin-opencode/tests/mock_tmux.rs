#![allow(clippy::expect_used)]

//! Tests for the `MockTmux` helper in `tests/common/mod.rs`.
//!
//! These exercise `discover_with_tmux` and `watcher::capture_pane`
//! against a fake `tmux` binary created in a tempdir. The fake
//! binary's `list-panes` and `capture-pane` outputs are configured
//! per-test via `MockTmux::with_output`.

mod common;

use std::collections::HashMap;

use agent_plugin_opencode::discovery::discover_with_tmux;
use agent_plugin_opencode::watcher::capture_pane;

use common::MockTmux;

#[tokio::test]
async fn discover_with_mock_tmux_returns_empty_for_bogus_panes() {
    let mock = MockTmux::with_output(
        "dev %0 1 /tmp/proj zsh\ndev %1 2 /tmp/other bash\n",
        &HashMap::new(),
    );
    let panes = discover_with_tmux(mock.path()).await.expect("discovery ok");
    assert!(
        panes.is_empty(),
        "expected no opencode panes (pane_current_command is zsh/bash, not opencode), got {panes:?}"
    );
}

#[tokio::test]
async fn discover_with_mock_tmux_returns_empty_when_list_is_empty() {
    let mock = MockTmux::new();
    let panes = discover_with_tmux(mock.path()).await.expect("discovery ok");
    assert!(panes.is_empty(), "expected empty result, got {panes:?}");
}

#[tokio::test]
async fn capture_pane_returns_configured_output_for_pane() {
    let mut capture = HashMap::new();
    capture.insert(
        "%0".to_string(),
        "Compiling foo v0.1.0\nBuild complete. \u{276f}\n".to_string(),
    );
    let mock = MockTmux::with_output("", &capture);
    let captured = capture_pane("%0", 50, mock.path()).await;
    assert!(
        captured.contains("Compiling foo v0.1.0"),
        "expected captured pane to include working keyword, got: {captured:?}"
    );
    assert!(
        captured.contains("Build complete"),
        "expected captured pane to include idle prompt, got: {captured:?}"
    );
}

#[tokio::test]
async fn capture_pane_returns_empty_for_unknown_pane() {
    let mut capture = HashMap::new();
    capture.insert("%0".to_string(), "some content\n".to_string());
    let mock = MockTmux::with_output("", &capture);
    let captured = capture_pane("%99", 50, mock.path()).await;
    assert!(
        captured.is_empty(),
        "expected empty output for unknown pane, got: {captured:?}"
    );
}
