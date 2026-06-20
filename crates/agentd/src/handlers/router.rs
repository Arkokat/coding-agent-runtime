//! Sync JSON-RPC 2.0 router for the control UDS.
//!
//! `handle_client` is invoked by `ControlServer::serve` on a
//! `spawn_blocking` worker thread (one call per accepted connection).
//! It opens a fresh `Db` from `paths.state_db_path`, reads one
//! JSON-RPC request, dispatches it through the read/mutate handlers,
//! and writes the JSON-RPC response back on the stream.
//!
//! The DB is opened per request rather than shared across requests:
//! `Db` wraps a `rusqlite::Connection`, which is `!Sync` and cannot
//! be shared across the `spawn_blocking` workers via `Arc`. Opening
//! per request is cheap (one `open` + a few pragmas) and matches
//! the per-request connection model of the v1 single-writer design.

use crate::db::Db;
use crate::handlers::{mutate, read};
use crate::paths::Paths;
use crate::tmux::Tmux;
use agentd_protocol::ProtocolError;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

/// Handle one client connection: read one request, dispatch, write
/// one response. Best-effort: any I/O error or protocol violation
/// ends the connection (returns). The caller (`ControlServer::serve`)
/// accepts the next connection.
pub fn handle_client(stream: UnixStream, paths: &Paths, tmux: &dyn Tmux) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "control router: failed to clone stream for reading");
            return;
        }
    };
    let mut reader = BufReader::new(reader_stream);
    let mut writer = stream;

    let Some(req) = read_request(&mut reader) else {
        return; // EOF or framing error; close quietly
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = req.get("method").and_then(Value::as_str) else {
        let resp = json_rpc_error(&id, ProtocolError::InvalidRequest);
        let _ = write_response(&mut writer, &resp);
        return;
    };
    let method = method.to_string();
    let params = req.get("params").cloned().unwrap_or(Value::Null);

    let db = match Db::open(&paths.state_db_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(error = %e, "control router: failed to open db");
            let resp = json_rpc_error(&id, ProtocolError::InternalError);
            let _ = write_response(&mut writer, &resp);
            return;
        }
    };

    let resp = dispatch(&method, params, &db, tmux, &id);
    let _ = write_response(&mut writer, &resp);
}

/// Dispatch `method` through the read and mutate handlers. Returns
/// the JSON-RPC response object (success or error).
fn dispatch(method: &str, params: Value, db: &Db, tmux: &dyn Tmux, id: &Value) -> Value {
    if let Some(value) = read::dispatch(method, params.clone(), db) {
        return json_rpc_result(id, &value);
    }
    let mut_result = mutate::dispatch(method, params, db, tmux);
    match mut_result {
        mutate::MutateResult::Ok(v) => json_rpc_result(id, &v),
        mutate::MutateResult::Err(e) => json_rpc_error(id, e),
    }
}

/// Build a JSON-RPC success response with `result = value`.
fn json_rpc_result(id: &Value, result: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

/// Build a JSON-RPC error response from a `ProtocolError`.
fn json_rpc_error(id: &Value, err: ProtocolError) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": err.code(),
            "message": err.to_string(),
        },
    })
}

/// Read one NDJSON-encoded JSON-RPC request from `reader`. Returns
/// `None` on EOF (clean close) or on a framing/parse error (the
/// caller logs and drops the connection).
fn read_request<R: BufRead>(reader: &mut R) -> Option<Value> {
    let mut line = String::new();
    let n = match reader.read_line(&mut line) {
        Ok(0) => return None,
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(error = %e, "control router: read failed");
            return None;
        }
    };
    let _ = n;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(error = %e, "control router: invalid json");
            None
        }
    }
}

/// Serialize `value` as one NDJSON line and write to `writer`. The
/// error is logged but not returned (we're best-effort at this
/// point).
fn write_response<W: Write>(writer: &mut W, value: &Value) -> std::io::Result<()> {
    crate::ipc::framing::write_message(writer, value)
        .map_err(|e| std::io::Error::other(format!("framing: {e}")))
}
