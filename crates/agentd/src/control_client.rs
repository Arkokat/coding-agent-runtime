use crate::event_bus::Event;
use serde_json::{Value, json};
use std::path::Path;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream as TokioUnixStream;
use tokio::sync::broadcast;

#[derive(Debug, Error)]
pub enum ControlClientError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("rpc error {code}: {message}", code = .0.code, message = .0.message)]
    Rpc(agentd_protocol::ProtocolErrorWithMessage),
    #[error("connection closed")]
    Closed,
}

/// Minimal JSON-RPC client over a control UDS. One connection per call.
#[derive(Debug)]
pub struct ControlClient {
    path: std::path::PathBuf,
}

impl ControlClient {
    /// Just store the path. The actual connection happens in `call`.
    /// `async` signature required by callers in async contexts.
    #[allow(clippy::unused_async)]
    pub async fn connect(socket: &Path) -> Result<Self, ControlClientError> {
        Ok(Self {
            path: socket.to_path_buf(),
        })
    }

    /// Send one request, read one response. Uses a fresh connection per call.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, ControlClientError> {
        let stream = TokioUnixStream::connect(&self.path).await?;
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let id: u64 = 1;
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut buf = serde_json::to_vec(&req)?;
        buf.push(b'\n');
        write_half.write_all(&buf).await?;
        write_half.flush().await?;

        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(ControlClientError::Closed);
        }
        let resp: Value = serde_json::from_str(line.trim())?;
        if let Some(err) = resp.get("error") {
            let e: agentd_protocol::ProtocolErrorWithMessage = serde_json::from_value(err.clone())?;
            return Err(ControlClientError::Rpc(e));
        }
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Open a new connection to the daemon and subscribe to event
    /// notifications. Returns a `broadcast::Receiver<Event>` that yields
    /// every `event` notification the daemon sends. The background
    /// reader task is detached and lives until the daemon closes the
    /// connection (e.g. on `unsubscribe` or daemon shutdown); the
    /// receiver can be dropped without affecting the subscription.
    pub async fn subscribe(
        &self,
        filter: serde_json::Value,
    ) -> Result<broadcast::Receiver<Event>, ControlClientError> {
        // Open a new connection.
        let stream = TokioUnixStream::connect(&self.path).await?;
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        // Send subscribe request.
        let id: u64 = 1;
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "subscribe",
            "params": filter,
        });
        let mut buf = serde_json::to_vec(&req)?;
        buf.push(b'\n');
        write_half.write_all(&buf).await?;
        write_half.flush().await?;

        // Read subscribe response.
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let resp: Value = serde_json::from_str(line.trim())?;
        if let Some(err) = resp.get("error") {
            return Err(ControlClientError::Rpc(serde_json::from_value(
                err.clone(),
            )?));
        }
        let _ = resp.get("result"); // discard; we don't use the subscription id

        // Create the broadcast channel for events.
        let (tx, rx) = broadcast::channel::<Event>(1024);

        // Background reader task: demux frames.
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                let Ok(n) = reader.read_line(&mut line).await else {
                    break;
                };
                if n == 0 {
                    break;
                }
                let frame: Value = match serde_json::from_str(line.trim()) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                // If frame has `id`, it's a response to a request. We don't
                // expect any after subscribe, so log and skip.
                // If frame has no `id` and `method == "event"`, broadcast to subscribers.
                if frame.get("id").is_none() {
                    if let Some(event_value) = frame.get("params").cloned() {
                        if let Ok(event) = serde_json::from_value::<Event>(event_value) {
                            let _ = tx.send(event);
                        }
                    }
                }
            }
        });

        Ok(rx)
    }
}
