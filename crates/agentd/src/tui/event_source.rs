//! TUI-side helper: connect to the daemon's control UDS and immediately
//! subscribe to event push. Returns a `ControlClient` (for sending
//! request/response RPCs) and a `broadcast::Receiver<Event>` (for live
//! updates).

use crate::control_client::ControlClient;
use crate::event_bus::Event;
use anyhow::{Context, Result};
use std::path::Path;
use tokio::sync::broadcast;

/// Connect to the daemon at `socket` and subscribe to live events.
/// Returns the client (kept around so the connection stays open) and
/// a broadcast receiver that yields every event the daemon pushes.
pub async fn connect_and_subscribe(
    socket: &Path,
) -> Result<(ControlClient, broadcast::Receiver<Event>)> {
    let path = socket.to_path_buf();
    let client = ControlClient::connect(&path)
        .await
        .with_context(|| format!("connect to {}", path.display()))?;
    let rx = client
        .subscribe(serde_json::json!({"events": ["session.*", "plugin.*", "daemon.*"]}))
        .await
        .with_context(|| "subscribe")?;
    Ok((client, rx))
}
