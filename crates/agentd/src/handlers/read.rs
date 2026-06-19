#![allow(clippy::needless_pass_by_value)]

use crate::db::Db;
use crate::db::repo::{EventRepo, SessionRepo};
use crate::state::State;
use agentd_protocol::Method;
use serde_json::{Value, json};
use uuid::Uuid;

/// Dispatch a read-only JSON-RPC method. Returns:
///   - `Some(value)` with the successful result,
///   - `None` if the method is not a read-only method (router will try mutate).
///   - `None` for known methods with invalid params / missing entities; the
///     router (Task 18) will translate this into the appropriate
///     `ProtocolError` response.
pub fn dispatch(method: &str, params: Value, db: &Db) -> Option<Value> {
    match method {
        Method::STATE_SNAPSHOT => {
            let snap = State::capture(db).ok()?;
            Some(json!({
                "sessions": snap.sessions,
                "plugins": snap.plugins,
            }))
        }
        Method::SESSION_GET => {
            let id = params.get("id")?.as_str()?;
            let id = Uuid::parse_str(id).ok()?;
            let s = SessionRepo::new(db).get(&id).ok()??;
            Some(serde_json::to_value(s).ok()?)
        }
        Method::SESSION_EVENTS => {
            let id = params.get("id")?.as_str()?;
            let id = Uuid::parse_str(id).ok()?;
            let events = EventRepo::new(db).list_for_session(&id).ok()?;
            Some(json!({ "events": events }))
        }
        Method::DAEMON_STATUS => Some(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_secs": 0u64,
            "session_count": SessionRepo::new(db).list().ok()?.len(),
        })),
        Method::PLUGIN_LIST => {
            let list = crate::db::repo::PluginRepo::new(db).list().ok()?;
            Some(serde_json::to_value(list).ok()?)
        }
        Method::METRICS => Some(json!({"status": "not yet implemented"})),
        _ => None,
    }
}
