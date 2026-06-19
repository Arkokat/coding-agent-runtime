use serde_json::{Value, json};
use std::os::unix::net::UnixStream;
use std::path::Path;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream as TokioUnixStream;

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
pub struct ControlClient {
    path: std::path::PathBuf,
}

impl ControlClient {
    #[allow(clippy::unused_async)] // async signature required by callers in async contexts
    pub async fn connect(socket: &Path) -> Result<Self, ControlClientError> {
        let _ = UnixStream::connect(socket)?;
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
}
