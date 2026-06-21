//! CLI flag tests for `agentd-plugin-opencode`.
//!
//! Verifies the rename from `--socket` to `--control-socket`,
//! matching the daemon's invocation in `plugin_spawner.rs`.

#![allow(clippy::expect_used)]

use agent_plugin_opencode::Cli;
use clap::Parser;

#[test]
fn control_socket_flag_is_recognized() {
    let cli = Cli::try_parse_from([
        "agentd-plugin-opencode",
        "--control-socket",
        "/tmp/test.sock",
    ])
    .expect("--control-socket should parse");
    assert_eq!(cli.control_socket.to_str().unwrap(), "/tmp/test.sock");
}

#[test]
fn old_socket_flag_is_rejected() {
    let result = Cli::try_parse_from(["agentd-plugin-opencode", "--socket", "/tmp/test.sock"]);
    assert!(
        result.is_err(),
        "old --socket flag should be rejected after rename"
    );
}
