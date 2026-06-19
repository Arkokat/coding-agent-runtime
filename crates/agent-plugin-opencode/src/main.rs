//! agentd-plugin-opencode — reference plugin.
//!
//! This is the v1 reference: it does not actually spawn an `opencode`
//! process. It demonstrates how a real plugin is structured: connect
//! to the plugin UDS, say hello, then forward events from a backend.
//!
//! For testing, the plugin runs in `--mock` mode and emits a scripted
//! sequence. In real mode (the default), it reads NDJSON events from
//! stdin and forwards each to the daemon.

use std::path::PathBuf;

use agentd_plugin_sdk::{AgentdClient, Backend, Event, MockBackend, RealBackend};
use anyhow::Result;
use clap::Parser;
use tokio::io::BufReader;

#[derive(Parser, Debug)]
#[command(
    name = "agentd-plugin-opencode",
    version,
    about = "Reference agentd plugin for opencode-style events"
)]
struct Cli {
    /// Path to the plugin UDS to connect to.
    #[arg(long, env = "AGENTD_PLUGIN_SOCKET")]
    socket: PathBuf,

    /// Run in mock mode: emit a scripted sequence and exit.
    #[arg(long)]
    mock: bool,

    /// Skip the `plugin.hello` call (for tests).
    #[arg(long)]
    no_hello: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();
    let mut client = AgentdClient::connect(&cli.socket).await?;
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
