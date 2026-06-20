use agentd_protocol::{AgentType, Session, SessionSource, SessionStatus};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

/// Build a representative `Session` for tests, with a single name and status.
///
/// Field values are picked to look plausible (e.g. non-zero context
/// tokens, a `tmux_session`) without modelling any real session data.
pub fn sample_session(id: Uuid, name: &str, status: SessionStatus) -> Session {
    Session {
        id,
        agent_type: AgentType::Opencode,
        working_dir: "/tmp/x".into(),
        tmux_session: Some(name.to_string()),
        tmux_pane_id: Some("%1".into()),
        display_name: name.to_string(),
        status,
        current_task: Some("task".into()),
        model: None,
        context_used_tokens: Some(42000),
        context_total_tokens: Some(200_000),
        cost_usd: Some(0.12),
        source: SessionSource::Cli,
        created_at: Utc::now(),
        last_event_at: None,
        finished_at: None,
        metadata: json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_session_uses_supplied_id_name_status() {
        let id = Uuid::now_v7();
        let s = sample_session(id, "alpha", SessionStatus::Working);
        assert_eq!(s.id, id);
        assert_eq!(s.display_name, "alpha");
        assert_eq!(s.tmux_session.as_deref(), Some("alpha"));
        assert_eq!(s.status, SessionStatus::Working);
    }

    #[test]
    fn sample_session_roundtrips_through_json() {
        let s = sample_session(Uuid::now_v7(), "beta", SessionStatus::Idle);
        let v = serde_json::to_value(&s).unwrap();
        let s2: Session = serde_json::from_value(v).unwrap();
        assert_eq!(s.id, s2.id);
        assert_eq!(s.display_name, s2.display_name);
        assert_eq!(s.status, s2.status);
    }
}
