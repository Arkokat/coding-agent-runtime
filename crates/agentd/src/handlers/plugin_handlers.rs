#![allow(
    clippy::needless_pass_by_value,
    clippy::items_after_statements,
    clippy::enum_glob_use,
    clippy::unnecessary_wraps
)]

use crate::db::Db;
use crate::db::repo::{EventRepo, SessionRepo};
use agentd_protocol::{AgentType, Method, ProtocolError, Session, SessionSource, SessionStatus};
use serde_json::{Value, json};
use uuid::Uuid;

/// Outcome of a plugin-side handler. Like `MutateResult` but scoped to
/// plugin UDS. Success and error variants carry different types.
pub type PluginResult = Result<Value, ProtocolError>;

/// Dispatch a plugin-side JSON-RPC method. `plugin_name` is the
/// authenticated plugin (server has already verified it's in the
/// allowlist before this is called).
pub fn dispatch(method: &str, params: Value, db: &Db, plugin_name: &str) -> PluginResult {
    match method {
        Method::PLUGIN_HELLO => plugin_hello(params, db, plugin_name),
        Method::SESSION_DISCOVER => session_discover(params, db, plugin_name),
        Method::SESSION_REPORT_EVENT => session_report_event(params, db, plugin_name),
        Method::PLUGIN_HEARTBEAT => plugin_heartbeat(),
        Method::PLUGIN_BYE => plugin_bye(),
        _ => Err(ProtocolError::MethodNotFound),
    }
}

fn plugin_hello(params: Value, db: &Db, plugin_name: &str) -> PluginResult {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or(ProtocolError::InvalidParams)?;
    if name != plugin_name {
        return Err(ProtocolError::PluginNotAllowed);
    }
    let _ = params.get("version");
    let _ = params.get("pid");
    // Update last_connected_at on the plugin row (inserts a new row if not present).
    let socket_name = format!("{name}.sock");
    let binary = format!("agentd-plugin-{name}");
    let _ = crate::db::repo::PluginRepo::new(db).upsert(name, &binary, &socket_name, true);
    let _ = crate::db::repo::PluginRepo::new(db).set_last_connected(name, chrono::Utc::now());
    Ok(json!({
        "ok": true,
        "plugin_id": name,
        "heartbeat_interval_secs": 5,
    }))
}

fn session_discover(params: Value, db: &Db, plugin_name: &str) -> PluginResult {
    let tmux_session = params
        .get("tmux_session")
        .and_then(Value::as_str)
        .ok_or(ProtocolError::InvalidParams)?;
    let tmux_pane_id = params
        .get("tmux_pane_id")
        .and_then(Value::as_str)
        .ok_or(ProtocolError::InvalidParams)?;
    let working_dir = params
        .get("working_dir")
        .and_then(Value::as_str)
        .ok_or(ProtocolError::InvalidParams)?;

    // Spec section 2: discovered sessions get `source = discovered`.
    // The agent_type is determined by the discovering plugin's name; v1
    // is a 1:1 mapping (plugin name = agent type).
    let agent_type = match plugin_name {
        "opencode" => AgentType::Opencode,
        "claude-code" => AgentType::ClaudeCode,
        "codex" => AgentType::Codex,
        "aider" => AgentType::Aider,
        _ => return Err(ProtocolError::PluginNotAllowed),
    };

    let id = Uuid::now_v7();
    let session = Session {
        id,
        agent_type,
        working_dir: working_dir.to_string(),
        tmux_session: Some(tmux_session.to_string()),
        tmux_pane_id: Some(tmux_pane_id.to_string()),
        display_name: std::path::Path::new(working_dir)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("session")
            .to_string(),
        status: SessionStatus::Starting,
        current_task: None,
        model: None,
        context_used_tokens: None,
        context_total_tokens: None,
        cost_usd: None,
        source: SessionSource::Discovered,
        created_at: chrono::Utc::now(),
        last_event_at: None,
        finished_at: None,
        metadata: json!({"plugin": plugin_name}),
    };
    SessionRepo::new(db)
        .insert(&session)
        .map_err(|_| ProtocolError::InternalError)?;
    Ok(json!({"ok": true, "session_id": id.to_string()}))
}

fn session_report_event(params: Value, db: &Db, _plugin_name: &str) -> PluginResult {
    let session_id_str = params
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or(ProtocolError::InvalidParams)?;
    let kind = params
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ProtocolError::InvalidParams)?;
    let payload = params.get("payload").cloned().unwrap_or(Value::Null);
    let _ts = params.get("ts");
    let id = Uuid::parse_str(session_id_str).map_err(|_| ProtocolError::InvalidParams)?;

    let session = SessionRepo::new(db)
        .get(&id)
        .map_err(|_| ProtocolError::InternalError)?
        .ok_or(ProtocolError::SessionNotFound)?;

    EventRepo::new(db)
        .insert(&id, kind, &payload)
        .map_err(|_| ProtocolError::InternalError)?;

    // Side effects per kind (spec section 9 lifecycle table).
    use agentd_protocol::SessionStatus::*;
    if kind == "session.status_changed" {
        if let Some(s) = payload.get("status").and_then(Value::as_str) {
            let new = match s {
                "idle" => Idle,
                "working" => Working,
                "waiting_for_user" => WaitingForUser,
                "errored" => Errored,
                "finished" => Finished,
                "starting" => Starting,
                _ => return Err(ProtocolError::InvalidParams),
            };
            SessionRepo::new(db)
                .update_status(&id, new)
                .map_err(|_| ProtocolError::InternalError)?;
        }
    } else if kind == "session.task_changed" {
        if let Some(task) = payload.get("task").and_then(Value::as_str) {
            SessionRepo::new(db)
                .update_task(&id, Some(task))
                .map_err(|_| ProtocolError::InternalError)?;
        }
    } else if kind == "session.usage_updated" {
        let used = payload.get("used").and_then(Value::as_u64);
        let total = payload.get("total").and_then(Value::as_u64);
        let cost = payload.get("cost_usd").and_then(Value::as_f64);
        SessionRepo::new(db)
            .update_usage(&id, used, total, cost)
            .map_err(|_| ProtocolError::InternalError)?;
    } else if kind == "session.finished" {
        SessionRepo::new(db)
            .mark_finished(&id)
            .map_err(|_| ProtocolError::InternalError)?;
    }
    let _ = session; // ownership rule (-32004) is enforced in Task 22 (supervisor)

    Ok(json!({"accepted": true, "event_id": id.to_string()}))
}

fn plugin_heartbeat() -> PluginResult {
    // Real counter wiring lands in Task 22 (per-plugin connection state).
    Ok(json!({
        "ok": true,
        "events_total": 0,
        "invalid_total": 0,
        "restart_required": false,
    }))
}

fn plugin_bye() -> PluginResult {
    Ok(json!({"ok": true}))
}
