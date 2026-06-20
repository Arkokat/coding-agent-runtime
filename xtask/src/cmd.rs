use std::process::{Command, exit};

/// Run a command, propagating its exit code.
fn run(mut cmd: Command) -> ! {
    let status = cmd.status().unwrap_or_else(|e| {
        eprintln!("xtask: failed to spawn: {e}");
        exit(127);
    });
    exit(status.code().unwrap_or(1))
}

/// `cargo fmt --all --check`
pub fn fmt() -> ! {
    let mut cmd = Command::new("cargo");
    cmd.args(["fmt", "--all", "--check"]);
    run(cmd)
}

/// `cargo clippy --workspace --all-targets -- -D warnings`
pub fn clippy() -> ! {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]);
    run(cmd)
}

/// `cargo nextest run --workspace` — local test runner, skips `#[ignore]`
/// tests (those need host features a local sandbox may block; see
/// `docs/superpowers/specs/2026-06-20-ci-only-test-tags-design.md`).
pub fn test() -> ! {
    let mut cmd = Command::new("cargo");
    cmd.args(["nextest", "run", "--workspace"]);
    run(cmd)
}

/// Run fmt → clippy → test. Fails fast. Always runs `#[ignore]` tests
/// via `--run-ignored all` so CI catches regressions in env-dependent
/// code paths.
#[allow(unreachable_code)] // each subcommand is `-> !`, so the chain is "unreachable" by lint rules
pub fn ci() -> ! {
    eprintln!("xtask: ci (fmt → clippy → test, including #[ignore])");
    fmt();
    clippy();
    let mut cmd = Command::new("cargo");
    cmd.args(["nextest", "run", "--run-ignored", "all", "--workspace"]);
    run(cmd)
}
