#![allow(
    clippy::unnecessary_wraps,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use anyhow::Result;
use clap::Parser;

pub mod cli;
pub mod config;
pub mod db;
pub mod handlers;
pub mod ipc;
pub mod paths;
pub mod plugins_manifest;
pub mod state;

use cli::{Cli, Command, DaemonAction, PluginAction};

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.quiet);
    match cli.command {
        Command::Daemon { action } => daemon(action),
        Command::List => println!("agentd list: not yet implemented"),
        Command::New { .. } => println!("agentd new: not yet implemented"),
        Command::Jump { id } => println!("agentd jump {id}: not yet implemented"),
        Command::Rename { id, name } => println!("agentd rename {id} {name}: not yet implemented"),
        Command::Kill { id } => println!("agentd kill {id}: not yet implemented"),
        Command::Status { global, pane } => status(global, pane),
        Command::Plugin { action } => plugin(action),
        Command::Init { .. } => println!("agentd init: not yet implemented"),
        Command::Doctor => println!("agentd doctor: not yet implemented"),
        Command::Metrics { format } => {
            println!("agentd metrics --format {format}: not yet implemented")
        }
        Command::Debug => println!("agentd debug: not yet implemented"),
        Command::Uninstall { .. } => println!("agentd uninstall: not yet implemented"),
    }
    Ok(())
}

fn init_tracing(quiet: bool) {
    use tracing_subscriber::{EnvFilter, fmt};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if quiet {
            EnvFilter::new("error")
        } else {
            EnvFilter::new("info")
        }
    });
    let _ = fmt().with_env_filter(filter).try_init();
}

fn daemon(action: DaemonAction) {
    match action {
        DaemonAction::Start { .. } => println!("agentd daemon start: not yet implemented"),
        DaemonAction::Stop => println!("agentd daemon stop: not yet implemented"),
        DaemonAction::Restart => println!("agentd daemon restart: not yet implemented"),
        DaemonAction::Status => println!("agentd daemon status: not yet implemented"),
    }
}

fn plugin(action: PluginAction) {
    match action {
        PluginAction::List => println!("agentd plugin list: not yet implemented"),
        PluginAction::Install { name } => {
            println!("agentd plugin install {name}: not yet implemented")
        }
        PluginAction::Update => println!("agentd plugin update: not yet implemented"),
        PluginAction::Remove { name } => {
            println!("agentd plugin remove {name}: not yet implemented")
        }
        PluginAction::Start { name } => println!("agentd plugin start {name}: not yet implemented"),
        PluginAction::Stop { name } => println!("agentd plugin stop {name}: not yet implemented"),
    }
}

fn status(global: bool, pane: Option<String>) {
    if global {
        println!("agentd status --global: not yet implemented");
    } else if let Some(p) = pane {
        println!("agentd status --pane {p}: not yet implemented");
    } else {
        println!("agentd status: specify --global or --pane <id>");
    }
}
