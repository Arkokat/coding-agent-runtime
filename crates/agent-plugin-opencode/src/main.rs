//! agentd-plugin-opencode — reference plugin.
//!
//! Three modes:
//! - `--watch` (default in real mode): discover opencode tmux panes
//!   and poll their status, emitting `session.discover` and
//!   `session.status_changed` events to the daemon.
//! - `--stdin`: NDJSON from stdin (backward compat).
//! - `--mock`: scripted events (backward compat, used in tests).
//!
//! Connects to the daemon over the UDS at `--control-socket`.

use std::path::Path;
use std::time::Duration;

use agent_plugin_opencode::Cli;
use agentd_plugin_sdk::{AgentdClient, Backend, Event, MockBackend, RealBackend};
use anyhow::Result;
use clap::Parser;
use tokio::io::BufReader;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    let mut client = AgentdClient::connect(&cli.control_socket).await?;
    if !cli.no_hello {
        let _ = client
            .hello(
                "opencode",
                env!("CARGO_PKG_VERSION"),
                std::process::id(),
                &std::env::current_exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
            )
            .await?;
    }
    let use_watch = cli.watch || (!cli.mock && !cli.stdin);
    let interval = Duration::from_millis(cli.poll_interval_ms);
    let pane_check_interval = Duration::from_millis(cli.pane_check_interval_ms);
    if use_watch {
        let tmux = Path::new("tmux");
        let panes = agent_plugin_opencode::discovery::discover_with_tmux(tmux).await?;
        agent_plugin_opencode::watcher::run(
            &mut client,
            panes,
            interval,
            pane_check_interval,
            tmux,
        )
        .await?;
        return Ok(());
    }
    if cli.mock {
        run_mock(&mut client).await
    } else {
        run_stdin(&mut client).await
    }
}

/// Initialize the global `tracing` subscriber for the plugin.
///
/// If `AGENTD_LOG_FILE` is set, tracing is written to that file (so
/// the daemon and its plugin children share one log). Otherwise the
/// default `tracing_subscriber::fmt` initializer writes to stderr
/// (which the daemon captures but discards if its own stdout is
/// `/dev/null`, so this path is best for foreground debugging).
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if let Ok(path) = std::env::var("AGENTD_LOG_FILE") {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(file)
                .with_ansi(false)
                .try_init();
            return;
        }
    }
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

async fn run_mock(client: &mut AgentdClient) -> Result<()> {
    let mut backend = MockBackend::new(vec![
        Event {
            kind: "session.started".into(),
            payload: serde_json::json!({}),
        },
        Event {
            kind: "session.status_changed".into(),
            payload: serde_json::json!({"status": "working"}),
        },
    ]);
    while let Some(event) = backend.next_event().await {
        if let Some(id) = event.payload.get("session_id").and_then(|v| v.as_str()) {
            if let Ok(uuid) = id.parse() {
                let _ = client
                    .report_event(uuid, &event.kind, event.payload.clone())
                    .await?;
            }
        }
        // Mock has no real session id; this is enough to exercise the SDK.
    }
    let _ = client.heartbeat().await?;
    let _ = client.bye().await?;
    Ok(())
}

#[allow(unused_imports, unused_mut, clippy::items_after_statements)] // silence pattern from brief; placeholder for v2 stdin reader
async fn run_stdin(client: &mut AgentdClient) -> Result<()> {
    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut backend = RealBackend::new(reader);
    let mut buf = String::new();
    use std::io::BufRead;
    let _ = buf; // silence unused
    while let Some(event) = backend.next_event().await {
        if event.kind == "exit" {
            break;
        }
        // Real plugin would track session_id from a discovered pane.
        // For the v1 reference, the daemon routes events by session_id
        // embedded in the payload if present, else drops them.
        if let Some(id) = event.payload.get("session_id").and_then(|v| v.as_str()) {
            if let Ok(uuid) = id.parse() {
                let _ = client
                    .report_event(uuid, &event.kind, event.payload.clone())
                    .await?;
            }
        }
    }
    let _ = client.bye().await?;
    Ok(())
}
