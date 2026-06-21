//! Integration tests for command-line matching in pane discovery.
//!
//! `pane_current_command` from `tmux list-panes` is just the process
//! basename (e.g. `node`), but a real opencode invocation launched
//! through nvm is `node /path/to/opencode.js run 'msg'`. The
//! `command_line_contains_opencode` helper is the substring check we
//! apply to the full `ps -o command=` output to catch that case.

use agent_plugin_opencode::discovery::command_line_contains_opencode;

#[test]
fn command_line_contains_opencode_matches_node_invocation() {
    let line = "node /Users/foo/.nvm/versions/node/v24.14.1/bin/../lib/node_modules/opencode/bin/opencode run 'msg'";
    assert!(command_line_contains_opencode(line));
}

#[test]
fn command_line_contains_opencode_matches_direct_invocation() {
    assert!(command_line_contains_opencode("opencode run 'msg'"));
}

#[test]
fn command_line_contains_opencode_rejects_other_commands() {
    assert!(!command_line_contains_opencode("bash"));
    assert!(!command_line_contains_opencode("vim"));
    assert!(!command_line_contains_opencode("node /some/other.js"));
}
