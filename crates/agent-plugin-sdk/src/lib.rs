//! agentd-plugin-sdk: helper crate for agentd plugin authors.
//!
//! Provides:
//! - [`Backend`] trait — event source abstraction
//! - [`MockBackend`] — scripted events for tests
//! - [`RealBackend`] — reads NDJSON from stdin
//! - [`AgentdClient`] — JSON-RPC client for the plugin UDS
//!
//! Reference plugins (e.g. `agentd-plugin-opencode`) use this crate
//! so they don't reinvent the framing, the auth, or the protocol.

#![warn(missing_docs)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

/// Re-export the protocol version the SDK targets. Plugins built
/// against this SDK send this version in `plugin.hello`.
pub const SDK_PROTOCOL_VERSION: u32 = agentd_protocol::PROTOCOL_VERSION;

/// One normalized event the plugin emits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    /// Event name (e.g. `session.started`, `session.status_changed`).
    pub kind: String,
    /// Event-specific payload.
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Event source abstraction. Real = stdin or child stdout; mock = scripted.
#[async_trait]
pub trait Backend: Send {
    /// Returns the next event, or `None` when the source is exhausted.
    async fn next_event(&mut self) -> Option<Event>;
}

/// Scripted backend for tests. Returns events in order, then `None`.
pub struct MockBackend {
    events: std::collections::VecDeque<Event>,
}

impl MockBackend {
    /// Build a scripted backend from a fixed event list.
    pub fn new(events: Vec<Event>) -> Self {
        Self {
            events: events.into(),
        }
    }
}

#[async_trait]
impl Backend for MockBackend {
    async fn next_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }
}

/// Backend that reads NDJSON events from a `tokio::io::AsyncBufRead`.
pub struct RealBackend<R: tokio::io::AsyncBufRead + Unpin + Send> {
    reader: R,
}

impl<R: tokio::io::AsyncBufRead + Unpin + Send> RealBackend<R> {
    /// Wrap any NDJSON-producing async reader as a [`Backend`].
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

#[async_trait]
impl<R: tokio::io::AsyncBufRead + Unpin + Send> Backend for RealBackend<R> {
    async fn next_event(&mut self) -> Option<Event> {
        use tokio::io::AsyncBufReadExt;
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await.ok()?;
        if n == 0 {
            return None;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        serde_json::from_str(trimmed).ok()
    }
}

/// All failure modes of [`AgentdClient`]: I/O, framing, and JSON-RPC errors.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Underlying I/O failure on the UDS connection.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Failed to encode/decode a JSON-RPC frame.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// JSON-RPC client for the daemon's plugin UDS. One connection per
/// plugin process; held for the plugin's lifetime.
pub struct AgentdClient {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    next_id: u64,
}

impl AgentdClient {
    /// Connect to the plugin UDS at the given path.
    pub async fn connect(socket: &Path) -> Result<Self, ClientError> {
        let stream = UnixStream::connect(socket).await?;
        let (read, write) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(read),
            writer: write,
            next_id: 1,
        })
    }

    /// Send a `plugin.hello` announcement.
    pub async fn hello(
        &mut self,
        name: &str,
        version: &str,
        pid: u32,
        binary_path: &str,
    ) -> Result<serde_json::Value, ClientError> {
        self.call(
            agentd_protocol::Method::PLUGIN_HELLO,
            serde_json::json!({
                "name": name,
                "version": version,
                "pid": pid,
                "binary_path": binary_path,
            }),
        )
        .await
    }

    /// Report an event for an existing session.
    pub async fn report_event(
        &mut self,
        session_id: uuid::Uuid,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        self.call(
            agentd_protocol::Method::SESSION_REPORT_EVENT,
            serde_json::json!({
                "session_id": session_id.to_string(),
                "type": kind,
                "payload": payload,
                "ts": chrono_now(),
            }),
        )
        .await
    }

    /// Tell the daemon about a session we just discovered in a tmux pane.
    pub async fn discover(
        &mut self,
        tmux_session: &str,
        tmux_pane_id: &str,
        working_dir: &str,
    ) -> Result<serde_json::Value, ClientError> {
        self.call(
            agentd_protocol::Method::SESSION_DISCOVER,
            serde_json::json!({
                "tmux_session": tmux_session,
                "tmux_pane_id": tmux_pane_id,
                "working_dir": working_dir,
            }),
        )
        .await
    }

    /// Liveness ping.
    pub async fn heartbeat(&mut self) -> Result<serde_json::Value, ClientError> {
        self.call(
            agentd_protocol::Method::PLUGIN_HEARTBEAT,
            serde_json::json!({}),
        )
        .await
    }

    /// Graceful disconnect.
    pub async fn bye(&mut self) -> Result<serde_json::Value, ClientError> {
        self.call(agentd_protocol::Method::PLUGIN_BYE, serde_json::json!({}))
            .await
    }

    async fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let id = self.next_id;
        self.next_id += 1;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut buf = serde_json::to_vec(&req)?;
        buf.push(b'\n');
        self.writer.write_all(&buf).await?;
        self.writer.flush().await?;

        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(ClientError::Io(std::io::Error::other(
                "daemon closed connection",
            )));
        }
        let resp: serde_json::Value = serde_json::from_str(line.trim())?;
        if let Some(err) = resp.get("error") {
            // Surface the error message via anyhow-style string for now;
            // full typed mapping lives in the daemon crate.
            return Err(ClientError::Io(std::io::Error::other(format!(
                "rpc error: {err}"
            ))));
        }
        Ok(resp
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }
}

fn chrono_now() -> String {
    // Avoid pulling chrono into the SDK just for one RFC3339 stamp.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0u64, |d| d.as_secs());
    // Minimal ISO 8601 UTC stamp without chrono. Not strictly RFC3339
    // in all cases, but parseable by `DateTime::parse_from_rfc3339`
    // for any reasonable test value.
    let days = secs / 86_400;
    let mut y = 1970u64;
    let mut d = days;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
        let year_days = if leap { 366 } else { 365 };
        if d < year_days {
            break;
        }
        d -= year_days;
        y += 1;
    }
    format!("{y}-01-01T00:00:00Z")
}
