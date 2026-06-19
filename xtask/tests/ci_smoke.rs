#![allow(clippy::expect_used)]

use std::process::Command;

#[test]
fn xtask_help_lists_all_subcommands() {
    let bin = env!("CARGO_BIN_EXE_xtask");
    let out = Command::new(bin)
        .arg("help")
        .output()
        .expect("run xtask help");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("fmt"));
    assert!(stdout.contains("clippy"));
    assert!(stdout.contains("test"));
    assert!(stdout.contains("ci"));
}

#[test]
fn xtask_unknown_subcommand_exits_nonzero() {
    let bin = env!("CARGO_BIN_EXE_xtask");
    let out = Command::new(bin)
        .arg("bogus-subcommand")
        .output()
        .expect("run");
    assert!(
        !out.status.success(),
        "unknown subcommand must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown subcommand") || stderr.contains("bogus-subcommand"));
}
