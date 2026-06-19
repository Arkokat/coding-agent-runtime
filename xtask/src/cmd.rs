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

/// `cargo test --workspace`
pub fn test() -> ! {
    let mut cmd = Command::new("cargo");
    cmd.args(["test", "--workspace"]);
    run(cmd)
}

/// Run fmt → clippy → test. Fails fast.
#[allow(unreachable_code)] // each subcommand is `-> !`, so the chain is "unreachable" by lint rules
pub fn ci() -> ! {
    eprintln!("xtask: ci (fmt → clippy → test)");
    fmt();
    clippy();
    test();
}
