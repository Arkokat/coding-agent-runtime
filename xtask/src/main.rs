//! xtask: build automation for agentd.
//!
//! Subcommands: `fmt`, `clippy`, `test`, `ci`, `help`.

mod cmd;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(String::as_str);
    match sub {
        Some("fmt") => cmd::fmt(),
        Some("clippy") => cmd::clippy(),
        Some("test") => cmd::test(),
        Some("ci") => cmd::ci(),
        Some("help" | "-h" | "--help") | None => print_help(),
        Some(other) => {
            eprintln!("xtask: unknown subcommand: {other}");
            eprintln!("Run `cargo xtask help` for the list.");
            std::process::exit(2);
        }
    }
}

fn print_help() -> ! {
    println!(
        "xtask {} — build automation for agentd",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    println!("Subcommands:");
    println!("  fmt       Run cargo fmt --all --check");
    println!("  clippy    Run cargo clippy --workspace --all-targets -- -D warnings");
    println!("  test      Run cargo test --workspace");
    println!("  ci        Run fmt + clippy + test in order (fail-fast)");
    println!("  help      Show this help");
    std::process::exit(0);
}
