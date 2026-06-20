use crate::db::Db;
use crate::ipc::framing;
use crate::plugin_heartbeat::HEARTBEAT_INTERVAL;
use agentd_protocol::ProtocolError;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::time::timeout;

/// All errors that can surface from the per-plugin UDS bind + handshake.
#[derive(Debug, Error)]
pub enum HandshakeError {
    /// Underlying `bind`, `accept`, or I/O on the accepted stream.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON framing problem (oversized line, invalid bytes, …).
    #[error("frame: {0}")]
    Frame(#[from] framing::FramingError),
    /// The handshake payload violated the protocol contract (missing
    /// `params.name`, etc.). Maps to a JSON-RPC `-32602` if the
    /// supervisor ever wants to surface it as a response.
    #[error("protocol: {0}")]
    Protocol(#[from] ProtocolError),
    /// No client connected (or the hello did not arrive) inside `wait`.
    #[error("timeout waiting for plugin.hello")]
    Timeout,
}

/// Parsed outcome of a successful `plugin.hello`. Returned by
/// `bind_and_handshake` to the supervisor; the supervisor decides
/// what to do with it (register the heartbeat, mark `connected`, etc.).
pub struct HandshakeResult {
    /// `params.name` from the hello. The supervisor will compare this
    /// against the allowlist (the same check `plugin_hello` does today
    /// via `dispatch`).
    pub plugin_name: String,
    /// `params.version` from the hello. May be empty when the plugin
    /// omits the field.
    pub version: String,
    /// `params.pid` from the hello, if the plugin sent it. Used by the
    /// supervisor's liveness checks (cross-checked against `SO_PEERCRED`).
    pub pid: Option<u32>,
}

/// Bind `socket` (0600 perms), accept the first connection, read a
/// `plugin.hello` request, write a canned JSON-RPC response, and return
/// the parsed hello fields. Times out after `wait`.
///
/// The handshake response is hard-coded (not dispatched through
/// `plugin_hello`): DB persistence of `last_connected_at` is the
/// supervisor's job — see `record_handshake` below for the DB-aware
/// helper, wired up in Task 9 (write-authority).
///
/// Idempotent: if a stale socket file exists, it is removed first so a
/// retried `bind` after a crashed supervisor doesn't fail with
/// `AddressInUse`.
pub async fn bind_and_handshake(
    socket: &Path,
    wait: Duration,
) -> Result<HandshakeResult, HandshakeError> {
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket)?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;

    let accepted = timeout(wait, listener.accept()).await;
    let (stream, _addr) = match accepted {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => return Err(HandshakeError::Io(e)),
        Err(_) => return Err(HandshakeError::Timeout),
    };
    let (r, mut w) = stream.into_split();
    let mut reader = BufReader::new(r);

    let mut line = String::new();
    let n = timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .map_err(|_| HandshakeError::Timeout)??;
    if n == 0 {
        return Err(HandshakeError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "client closed before plugin.hello",
        )));
    }
    let req: Value = serde_json::from_str(line.trim()).map_err(framing::FramingError::Json)?;
    let params = req
        .get("params")
        .cloned()
        .ok_or(ProtocolError::InvalidParams)?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or(ProtocolError::InvalidParams)?
        .to_string();
    let version = params
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let pid = params
        .get("pid")
        .and_then(Value::as_u64)
        .map(|p| u32::try_from(p).unwrap_or(u32::MAX));

    let resp = serde_json::json!({
        "jsonrpc": "2.0",
        "id": req.get("id").cloned().unwrap_or(serde_json::json!(1)),
        "result": {
            "ok": true,
            "plugin_id": name,
            "heartbeat_interval_secs": HEARTBEAT_INTERVAL.as_secs(),
        }
    });
    let mut buf: Vec<u8> = Vec::new();
    framing::write_message(&mut buf, &resp)?;
    w.write_all(&buf).await?;
    w.flush().await?;

    Ok(HandshakeResult {
        plugin_name: name,
        version,
        pid,
    })
}

/// Persist the post-handshake state. Call from the supervisor after
/// `bind_and_handshake` returns `Ok`, so `last_connected_at` reflects
/// the real connect (the same row is touched by `plugin_hello` in
/// `plugin_handlers`, but that path is for the post-boot IPC loop).
#[allow(dead_code, clippy::missing_errors_doc)]
pub fn record_handshake(db: &Db, name: &str) -> Result<(), crate::db::repo::RepoError> {
    use crate::db::repo::PluginRepo;
    let binary = format!("agentd-plugin-{name}");
    let socket_name = format!("{name}.sock");
    PluginRepo::new(db).upsert(name, &binary, &socket_name, true)?;
    PluginRepo::new(db).set_last_connected(name, chrono::Utc::now())
}
