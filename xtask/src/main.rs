fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "help" {
        println!(
            "xtask {} — build automation for agentd",
            env!("CARGO_PKG_VERSION")
        );
        println!();
        println!("Subcommands (added in later plans):");
        println!("  fmt           Run cargo fmt --check");
        println!("  clippy        Run cargo clippy --all-targets -- -D warnings");
        println!("  test          Run cargo nextest run --workspace");
        println!("  ci            Run fmt + clippy + test in order");
        println!("  release       Bump version, generate changelog, build release tarballs");
        return;
    }
    eprintln!("xtask: no subcommand given. Run `cargo xtask help`.");
    std::process::exit(1);
}
