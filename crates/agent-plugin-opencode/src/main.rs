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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
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
    if use_watch {
        let tmux = Path::new("tmux");
        let panes = agent_plugin_opencode::discovery::discover_with_tmux(tmux).await?;
        agent_plugin_opencode::watcher::run(&mut client, panes, interval, tmux).await?;
        return Ok(());
    }
    if cli.mock {
        run_mock(&mut client).await
    } else {
        run_stdin(&mut client).await
    }
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
