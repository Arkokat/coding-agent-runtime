#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

fn agentd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_agentd"))
}

#[test]
fn parses_daemon_start() {
    let out = Command::new(agentd_bin())
        .args(["daemon", "start", "--foreground"])
        .env("AGENTD_QUIET", "1")
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("daemon start"), "got: {stdout}");
}

#[test]
fn parses_new_with_cwd() {
    let out = Command::new(agentd_bin())
        .args(["new", "/tmp/agentd-cli-test", "--agent", "opencode"])
        .env("AGENTD_QUIET", "1")
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("agentd new"), "got: {stdout}");
}

#[test]
fn parses_status_pane() {
    let out = Command::new(agentd_bin())
        .args(["status", "--pane", "%5"])
        .env("AGENTD_QUIET", "1")
        .output()
        .expect("run");
    // With no running daemon and no DB, status --pane prints an empty line.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.trim().is_empty(), "got: {stdout:?}");
}

#[test]
fn status_global_and_pane_are_mutually_exclusive() {
    let out = Command::new(agentd_bin())
        .args(["status", "--global", "--pane", "%5"])
        .output()
        .expect("run");
    assert!(!out.status.success(), "clap must reject conflicting flags");
}

#[test]
fn version_flag_prints_crate_version() {
    let out = Command::new(agentd_bin())
        .arg("--version")
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), env!("CARGO_PKG_VERSION"));
}
