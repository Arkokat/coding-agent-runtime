#![allow(clippy::expect_used)]

use std::process::Command;

#[test]
fn help_flag_prints_usage() {
    let out = Command::new(env!("CARGO_BIN_EXE_agentd-plugin-opencode"))
        .arg("--help")
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("agentd-plugin-opencode"), "got: {stdout}");
    assert!(stdout.contains("--control-socket"), "got: {stdout}");
    assert!(stdout.contains("--mock"), "got: {stdout}");
    assert!(stdout.contains("--watch"), "got: {stdout}");
    assert!(stdout.contains("--stdin"), "got: {stdout}");
    assert!(stdout.contains("--poll-interval-ms"), "got: {stdout}");
}

#[test]
fn no_args_prints_help_and_exits_nonzero() {
    let out = Command::new(env!("CARGO_BIN_EXE_agentd-plugin-opencode"))
        .output()
        .expect("run");
    assert!(!out.status.success(), "no args should fail");
}
