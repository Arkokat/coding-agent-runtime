//! Sync JSON-RPC 2.0 router for the control UDS.
//!
//! `handle_client` is invoked by `ControlServer::serve` on a
//! `spawn_blocking` worker thread (one call per accepted connection).
//! It reads one JSON-RPC request from a `BufReader`, dispatches it
//! through the read/mutate handlers, and writes the response back.
//!
//! The DB is opened per request rather than shared across requests:
//! `Db` wraps a `rusqlite::Connection`, which is `!Sync` and cannot
//! be shared across the `spawn_blocking` workers via `Arc`. Opening
//! per request is cheap (one `open` + a few pragmas) and matches
//! the per-request connection model of the v1 single-writer design.
//!
//! `subscribe` is a streaming call: the handler writes the
//! `subscription_id` response, then loops forwarding bus events
//! from the supplied [`SubscriberRegistry`] until the client
//! disconnects. `unsubscribe` from a non-streaming connection is a
//! no-op for v1 (the registry removes the entry on stream close).

use crate::db::Db;
use crate::handlers::subscriber_registry::SubscriberRegistry;
use crate::handlers::{mutate, read};
use crate::paths::Paths;
use crate::tmux::Tmux;
use agentd_protocol::{Method, ProtocolError};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;
use tokio::sync::mpsc;

const SUBSCRIBE_PROBE_MS: u64 = 50;

/// Handle one client connection: read one request, dispatch, write
/// the response. For `subscribe`, after the ack response the
/// connection transitions to streaming mode and stays open until
/// the client disconnects (or the registry's bus goes quiet).
pub fn handle_client(
    stream: UnixStream,
    paths: &Paths,
    tmux: &dyn Tmux,
    registry: &SubscriberRegistry,
) {
    let writer_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "control router: failed to clone stream for writing");
            return;
        }
    };
    let mut writer = writer_stream;
    let mut reader = BufReader::new(stream);

    let Some(req) = read_request(&mut reader) else {
        return; // EOF or framing error; close quietly
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_string);
    let Some(method) = method else {
        let resp = json_rpc_error(&id, ProtocolError::InvalidRequest);
        let _ = write_response(&mut writer, &resp);
        return;
    };

    if method == Method::SUBSCRIBE {
        // Take a non-blocking clone of the read side; the BufReader's
        // small remaining buffer (if any) is intentionally dropped —
        // v1 protocol guarantees the subscribe request is a single
        // line, so the buffer should be empty here.
        let mut sub_stream = match reader.into_inner().try_clone() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "subscribe: clone read stream failed");
                return;
            }
        };
        if let Err(e) = sub_stream.set_nonblocking(true) {
            tracing::warn!(error = %e, "subscribe: set_nonblocking failed");
            return;
        }
        let mut sub_writer = match writer.try_clone() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "subscribe: clone writer failed");
                return;
            }
        };
        if let Err(e) = sub_writer.set_nonblocking(true) {
            tracing::warn!(error = %e, "subscribe: set_nonblocking writer failed");
            return;
        }
        run_subscribe(&mut sub_stream, &mut sub_writer, registry, &id, &req);
        return;
    }
    if method == Method::UNSUBSCRIBE {
        let resp = json_rpc_result(&id, &json!({"ok": true}));
        let _ = write_response(&mut writer, &resp);
        return;
    }

    let db = match Db::open(&paths.state_db_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(error = %e, "control router: failed to open db");
            let resp = json_rpc_error(&id, ProtocolError::InternalError);
            let _ = write_response(&mut writer, &resp);
            return;
        }
    };

    let resp = dispatch(
        &method,
        req.get("params").cloned().unwrap_or(Value::Null),
        &db,
        tmux,
        &id,
    );
    let _ = write_response(&mut writer, &resp);
}

/// Drive the streaming half of a `subscribe` request: write the ack,
/// then pump bus events to the client until it goes away.
fn run_subscribe(
    stream: &mut UnixStream,
    writer: &mut UnixStream,
    registry: &SubscriberRegistry,
    id: &Value,
    req: &Value,
) {
    let (sub_id, mut rx) = registry.register();
    let filter = req.get("params").cloned().unwrap_or(json!({}));
    let ack = json_rpc_result(
        id,
        &json!({
            "subscription_id": sub_id,
            "filter": filter,
        }),
    );
    if write_response(writer, &ack).is_err() {
        registry.unregister(sub_id);
        return;
    }
    loop {
        match read_line_nonblocking(stream) {
            ReadLine::Eof => break,
            ReadLine::WouldBlock => {}
            ReadLine::Error => {
                tracing::warn!("subscribe: client stream read error; closing");
                break;
            }
            ReadLine::Line(line) => {
                if is_unsubscribe_frame(&line) {
                    break;
                }
                // Non-subscribe frame on a subscribe connection:
                // ignore and continue (v1 is lenient; v2 should
                // return a JSON-RPC error over a separate channel).
            }
        }
        match rx.try_recv() {
            Ok(event) => {
                let frame = json!({
                    "jsonrpc": "2.0",
                    "method": Method::EVENT,
                    "params": event,
                });
                if write_response(writer, &frame).is_err() {
                    break;
                }
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(SUBSCRIBE_PROBE_MS));
            }
            Err(mpsc::error::TryRecvError::Disconnected) => break,
        }
    }
    registry.unregister(sub_id);
}

enum ReadLine {
    Line(String),
    Eof,
    WouldBlock,
    Error,
}

/// Read one NDJSON line from `stream` non-blockingly. Returns the
/// line (without trailing newline), `Eof` at clean close, `WouldBlock`
/// if no data is available right now, or `Error` for any other I/O
/// failure.
fn read_line_nonblocking(stream: &mut UnixStream) -> ReadLine {
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    let mut scratch = [0u8; 256];
    loop {
        match stream.read(&mut scratch) {
            Ok(0) => {
                if buf.is_empty() {
                    return ReadLine::Eof;
                }
                return ReadLine::Line(String::from_utf8_lossy(&buf).into_owned());
            }
            Ok(n) => {
                buf.extend_from_slice(&scratch[..n]);
                if let Some(idx) = buf.iter().position(|b| *b == b'\n') {
                    let line = String::from_utf8_lossy(&buf[..idx]).into_owned();
                    return ReadLine::Line(line);
                }
                if buf.len() > crate::ipc::framing::MAX_LINE_BYTES {
                    return ReadLine::Error;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if buf.is_empty() {
                    return ReadLine::WouldBlock;
                }
                return ReadLine::Line(String::from_utf8_lossy(&buf).into_owned());
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                // Spurious wakeup; loop and retry.
            }
            Err(_) => return ReadLine::Error,
        }
    }
}

/// True if `line` parses as a JSON-RPC request whose method is
/// `unsubscribe`.
fn is_unsubscribe_frame(line: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
        return false;
    };
    v.get("method")
        .and_then(Value::as_str)
        .is_some_and(|m| m == Method::UNSUBSCRIBE)
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
    // Bound the read to MAX_LINE_BYTES + 1: a client that sends a
    // multi-gigabyte line with no newline would otherwise make `line`
    // grow unbounded before the post-read check fired. Reading one
    // byte past the cap is enough to distinguish "exactly at the
    // limit" from "over the limit".
    let mut limited = BufReader::new(reader.take((crate::ipc::framing::MAX_LINE_BYTES as u64) + 1));
    let n = match limited.read_line(&mut line) {
        Ok(0) => return None,
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(error = %e, "control router: read failed");
            return None;
        }
    };
    if n > crate::ipc::framing::MAX_LINE_BYTES {
        tracing::warn!(bytes = n, "control router: line too long");
        return None;
    }
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
