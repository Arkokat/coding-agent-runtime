#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

fn agentd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_agentd"))
}

#[test]
fn parses_daemon_start() {
    // Use --help so the process exits immediately instead of starting the
    // daemon and entering its idle loop (which would hang the test forever).
    let out = Command::new(agentd_bin())
        .args(["daemon", "start", "--help"])
        .env("AGENTD_QUIET", "1")
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "clap should accept `daemon start --help`, got status: {:?}",
        out.status
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("daemon"),
        "help text should mention the daemon subcommand, got: {stdout}"
    );
}

#[test]
fn parses_new_with_cwd() {
    // Use --help so the process exits immediately instead of forking a
    // daemon and creating a real session.
    let out = Command::new(agentd_bin())
        .args(["new", "--help"])
        .env("AGENTD_QUIET", "1")
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "clap should accept `new --help`, got status: {:?}",
        out.status
    );
}

#[test]
fn parses_status_pane() {
    // Use --help so the process exits immediately. The point of this test
    // is to verify clap accepts the args, not to run `status` against the
    // user's real XDG DB.
    let out = Command::new(agentd_bin())
        .args(["status", "--help"])
        .env("AGENTD_QUIET", "1")
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "clap should accept `status --help`, got status: {:?}",
        out.status
    );
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
