//! CLI definition for the `agentd-plugin-opencode` binary.

use std::path::PathBuf;

use clap::Parser;

/// Command-line arguments for the `agentd-plugin-opencode` binary.
#[derive(Parser, Debug)]
#[command(
    name = "agentd-plugin-opencode",
    version,
    about = "Reference agentd plugin for opencode-style events"
)]
#[allow(clippy::struct_excessive_bools)] // clap idiom: one bool per flag
pub struct Cli {
    /// Path to the plugin UDS to connect to.
    #[arg(long, env = "AGENTD_PLUGIN_CONTROL_SOCKET")]
    pub control_socket: PathBuf,

    /// Run in watch mode: discover opencode tmux panes and poll for
    /// status. This is the default mode.
    #[arg(long)]
    pub watch: bool,

    /// Run in mock mode: emit a scripted sequence and exit.
    #[arg(long)]
    pub mock: bool,

    /// Read NDJSON events from stdin (legacy mode).
    #[arg(long)]
    pub stdin: bool,

    /// Polling interval for watch mode, in milliseconds.
    #[arg(long, default_value = "2000", env = "AGENTD_OPENCODE_POLL_MS")]
    pub poll_interval_ms: u64,

    /// Skip the `plugin.hello` call (for tests).
    #[arg(long)]
    pub no_hello: bool,
}
