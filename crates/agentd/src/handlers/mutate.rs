#![allow(
    clippy::needless_pass_by_value,
    clippy::manual_let_else,
    clippy::match_wildcard_for_single_variants,
    clippy::map_unwrap_or
)]

use crate::db::Db;
use crate::db::repo::SessionRepo;
use crate::tmux::Tmux;
use agentd_protocol::{AgentType, Method, ProtocolError, Session, SessionSource, SessionStatus};
use serde_json::{Value, json};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use uuid::Uuid;

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Was `daemon.shutdown` called by any client?
pub fn shutdown_requested() -> bool {
    SHUTDOWN.load(Ordering::SeqCst)
}

/// Test-only: reset the flag between tests.
pub fn reset_shutdown_for_tests() {
    SHUTDOWN.store(false, Ordering::SeqCst);
}

/// Result of dispatching a mutating method. The router converts
/// this to a JSON-RPC success or error response.
pub enum MutateResult {
    Ok(Value),
    Err(ProtocolError),
}

impl serde::Serialize for MutateResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            MutateResult::Ok(v) => v.serialize(serializer),
            MutateResult::Err(e) => e.serialize(serializer),
        }
    }
}

impl MutateResult {
    pub fn into_value(self) -> Option<Value> {
        match self {
            MutateResult::Ok(v) => Some(v),
            _ => None,
        }
    }
    pub fn into_err(self) -> Option<ProtocolError> {
        match self {
            MutateResult::Err(e) => Some(e),
            _ => None,
        }
    }
}

/// Dispatch a mutating JSON-RPC method. Returns `MutateResult::Err(MethodNotFound)`
/// for methods that are not mutations (router should try the read dispatcher).
pub fn dispatch(method: &str, params: Value, db: &Db, tmux: &dyn Tmux) -> MutateResult {
    match method {
        Method::SESSION_CREATE => session_create(params, db),
        Method::SESSION_RENAME => session_rename(params, db),
        Method::SESSION_DISMISS_ERROR => session_dismiss_error(params, db),
        Method::SESSION_JUMP => session_jump(params, db, tmux),
        Method::SESSION_KILL => session_kill(params, db, tmux),
        Method::DAEMON_SHUTDOWN => {
            SHUTDOWN.store(true, Ordering::SeqCst);
            MutateResult::Ok(json!({"ok": true}))
        }
        _ => MutateResult::Err(ProtocolError::MethodNotFound),
    }
}

fn session_create(params: Value, db: &Db) -> MutateResult {
    let agent_type = match params.get("agent_type").and_then(Value::as_str) {
        Some(s) => match s {
            "opencode" => AgentType::Opencode,
            "claude-code" => AgentType::ClaudeCode,
            "codex" => AgentType::Codex,
            "aider" => AgentType::Aider,
            _ => return MutateResult::Err(ProtocolError::InvalidParams),
        },
        None => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    let working_dir = match params.get("working_dir").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    let display_name = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            std::path::Path::new(&working_dir)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("session")
                .to_string()
        });

    let session = Session {
        id: Uuid::now_v7(),
        agent_type,
        working_dir,
        tmux_session: None,
        tmux_pane_id: None,
        display_name,
        status: SessionStatus::Starting,
        current_task: None,
        model: None,
        context_used_tokens: None,
        context_total_tokens: None,
        cost_usd: None,
        source: SessionSource::Cli,
        created_at: chrono::Utc::now(),
        last_event_at: None,
        finished_at: None,
        metadata: serde_json::json!({}),
    };
    if SessionRepo::new(db).insert(&session).is_err() {
        return MutateResult::Err(ProtocolError::InternalError);
    }
    match serde_json::to_value(&session) {
        Ok(v) => MutateResult::Ok(v),
        Err(_) => MutateResult::Err(ProtocolError::InternalError),
    }
}

fn session_rename(params: Value, db: &Db) -> MutateResult {
    let id_str = match params.get("id").and_then(Value::as_str) {
        Some(s) => s,
        None => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    let name = match params.get("name").and_then(Value::as_str) {
        Some(s) => s,
        None => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    let id = match Uuid::parse_str(id_str) {
        Ok(u) => u,
        Err(_) => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    if SessionRepo::new(db).update_rename(&id, name).is_err() {
        return MutateResult::Err(ProtocolError::SessionNotFound);
    }
    match SessionRepo::new(db).get(&id) {
        Ok(Some(s)) => match serde_json::to_value(s) {
            Ok(v) => MutateResult::Ok(v),
            Err(_) => MutateResult::Err(ProtocolError::InternalError),
        },
        _ => MutateResult::Err(ProtocolError::SessionNotFound),
    }
}

fn session_dismiss_error(params: Value, db: &Db) -> MutateResult {
    let id_str = match params.get("id").and_then(Value::as_str) {
        Some(s) => s,
        None => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    let id = match Uuid::parse_str(id_str) {
        Ok(u) => u,
        Err(_) => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    let session = match SessionRepo::new(db).get(&id) {
        Ok(Some(s)) => s,
        _ => return MutateResult::Err(ProtocolError::SessionNotFound),
    };
    if session.status != SessionStatus::Errored {
        return MutateResult::Err(ProtocolError::InvalidParams);
    }
    if SessionRepo::new(db)
        .update_status(&id, SessionStatus::Idle, chrono::Utc::now())
        .is_err()
    {
        return MutateResult::Err(ProtocolError::InternalError);
    }
    match SessionRepo::new(db).get(&id) {
        Ok(Some(s)) => match serde_json::to_value(s) {
            Ok(v) => MutateResult::Ok(v),
            Err(_) => MutateResult::Err(ProtocolError::InternalError),
        },
        _ => MutateResult::Err(ProtocolError::InternalError),
    }
}

/// Handler for `session.jump`: switch the connected tmux client to the
/// session's tmux window. Returns `SessionNotFound` for missing sessions
/// or missing tmux bindings, and surfaces tmux errors as `SessionNotFound`.
fn session_jump(params: Value, db: &Db, tmux: &dyn Tmux) -> MutateResult {
    let id_str = match params.get("id").and_then(Value::as_str) {
        Some(s) => s,
        None => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    let id = match Uuid::parse_str(id_str) {
        Ok(u) => u,
        Err(_) => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    let session = match SessionRepo::new(db).get(&id) {
        Ok(Some(s)) => s,
        _ => return MutateResult::Err(ProtocolError::SessionNotFound),
    };
    let target = match session.tmux_session {
        Some(t) => t,
        None => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    match futures::executor::block_on(tmux.switch_client(&target)) {
        Ok(()) => MutateResult::Ok(json!({"ok": true})),
        Err(_) => MutateResult::Err(ProtocolError::SessionNotFound),
    }
}

/// Handler for `session.kill`: tear down the session's tmux window
/// (if any) and mark the row `finished`. Returns the updated session on
/// success, or `SessionNotFound` for missing or already-finished sessions.
fn session_kill(params: Value, db: &Db, tmux: &dyn Tmux) -> MutateResult {
    let id_str = match params.get("id").and_then(Value::as_str) {
        Some(s) => s,
        None => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    let id = match Uuid::parse_str(id_str) {
        Ok(u) => u,
        Err(_) => return MutateResult::Err(ProtocolError::InvalidParams),
    };
    let session = match SessionRepo::new(db).get(&id) {
        Ok(Some(s)) => s,
        _ => return MutateResult::Err(ProtocolError::SessionNotFound),
    };
    if session.status.is_terminal() {
        return MutateResult::Err(ProtocolError::SessionNotFound);
    }
    if let Some(target) = &session.tmux_session {
        let _ = futures::executor::block_on(tmux.kill_session(target));
    }
    if SessionRepo::new(db)
        .mark_finished(&id, chrono::Utc::now())
        .is_err()
    {
        return MutateResult::Err(ProtocolError::InternalError);
    }
    match SessionRepo::new(db).get(&id) {
        Ok(Some(s)) => match serde_json::to_value(s) {
            Ok(v) => MutateResult::Ok(v),
            Err(_) => MutateResult::Err(ProtocolError::InternalError),
        },
        _ => MutateResult::Err(ProtocolError::InternalError),
    }
}
