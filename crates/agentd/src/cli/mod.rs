pub mod init;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// agentd: orchestrate coding-agent sessions inside tmux.
#[derive(Debug, Parser)]
#[command(name = "agentd", display_name = "", version, about, long_about = None)]
pub struct Cli {
    /// Path to config file (default: `$XDG_CONFIG_HOME/agentd/config.toml`).
    #[arg(long, global = true, env = "AGENTD_CONFIG")]
    pub config: Option<PathBuf>,

    /// Suppress non-error output.
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Disable colored output.
    #[arg(long, global = true)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start, stop, or check the daemon.
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// List active sessions.
    List,
    /// Create a new session.
    New {
        /// Working directory for the new session. Defaults to $PWD.
        cwd: Option<String>,
        /// Always show interactive picker, even when path is given.
        #[arg(long)]
        pick: bool,
        /// Show most-recent paths and pick by number.
        #[arg(long)]
        recent: bool,
        /// Override the default agent.
        #[arg(long)]
        agent: Option<String>,
    },
    /// Switch the tmux client to a session's window.
    Jump {
        /// Session id (UUID v7 prefix is fine).
        id: String,
    },
    /// Rename a session.
    Rename {
        /// Session id.
        id: String,
        /// New display name.
        name: String,
    },
    /// Kill a session and remove its tmux pane.
    Kill {
        /// Session id.
        id: String,
    },
    /// Show status line output (used by tmux `status-interval`).
    Status {
        /// Aggregate line: "5 agents · 2 waiting · 1 working · $0.42".
        #[arg(long, conflicts_with = "pane")]
        global: bool,
        /// Per-pane line for the given pane id (e.g. `%5`).
        #[arg(long, conflicts_with = "global")]
        pane: Option<String>,
    },
    /// Manage plugins.
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
    /// First-run setup: create config, install tmux fragment.
    Init {
        /// Skip the interactive confirmation prompts.
        #[arg(long)]
        yes: bool,
    },
    /// Check daemon health and config validity.
    Doctor,
    /// Show current metrics.
    Metrics {
        /// Output format.
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Generate a debug bundle tarball.
    Debug,
    /// Remove agentd config, state, and runtime files.
    Uninstall {
        /// Don't prompt for confirmation.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum DaemonAction {
    /// Start the daemon (default: detach).
    Start {
        /// Run in the foreground (don't fork).
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the running daemon.
    Stop,
    /// Stop, then start.
    Restart,
    /// Print daemon status and exit.
    Status,
}

#[derive(Debug, Subcommand)]
pub enum PluginAction {
    /// List installed plugins.
    List,
    /// Download a plugin binary.
    Install {
        /// Plugin name (e.g. "opencode").
        name: String,
    },
    /// Update all installed plugins.
    Update,
    /// Remove an installed plugin.
    Remove {
        /// Plugin name.
        name: String,
    },
    /// Start a configured plugin.
    Start {
        /// Plugin name.
        name: String,
    },
    /// Stop a running plugin.
    Stop {
        /// Plugin name.
        name: String,
    },
}
