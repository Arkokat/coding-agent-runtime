use agentd_protocol::{AgentType, Session, SessionSource, SessionStatus};
use chrono::Utc;
use uuid::Uuid;

fn sample_session() -> Session {
    Session {
        id: Uuid::now_v7(),
        agent_type: AgentType::Opencode,
        working_dir: "/Users/me/projects/agentd".into(),
        tmux_session: Some("agentd-01HXYZ".into()),
        tmux_pane_id: Some("%5".into()),
        display_name: "agentd".into(),
        status: SessionStatus::Working,
        current_task: Some("editing src/main.rs".into()),
        model: Some("claude-sonnet-4-5".into()),
        context_used_tokens: Some(42000),
        context_total_tokens: Some(200_000),
        cost_usd: Some(0.12),
        source: SessionSource::Cli,
        created_at: Utc::now(),
        last_event_at: Some(Utc::now()),
        finished_at: None,
        metadata: serde_json::json!({"git_branch": "main"}),
    }
}

#[test]
fn session_roundtrips_through_json() {
    let original = sample_session();
    let json = serde_json::to_string(&original).unwrap();
    let parsed: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(original.id, parsed.id);
    assert_eq!(original.agent_type, parsed.agent_type);
    assert_eq!(original.working_dir, parsed.working_dir);
    assert_eq!(original.tmux_session, parsed.tmux_session);
    assert_eq!(original.status, parsed.status);
    assert_eq!(original.current_task, parsed.current_task);
    assert_eq!(original.model, parsed.model);
    assert_eq!(original.context_used_tokens, parsed.context_used_tokens);
    assert_eq!(original.cost_usd, parsed.cost_usd);
    assert_eq!(original.source, parsed.source);
    assert_eq!(original.metadata, parsed.metadata);
}

#[test]
fn agent_type_serializes_as_kebab_case_string() {
    assert_eq!(
        serde_json::to_string(&AgentType::ClaudeCode).unwrap(),
        r#""claude-code""#
    );
    assert_eq!(
        serde_json::to_string(&AgentType::Opencode).unwrap(),
        r#""opencode""#
    );
    assert_eq!(
        serde_json::to_string(&AgentType::Codex).unwrap(),
        r#""codex""#
    );
    assert_eq!(
        serde_json::to_string(&AgentType::Aider).unwrap(),
        r#""aider""#
    );
}

#[test]
fn session_source_serializes_as_snake_case_string() {
    assert_eq!(
        serde_json::to_string(&SessionSource::Cli).unwrap(),
        r#""cli""#
    );
    assert_eq!(
        serde_json::to_string(&SessionSource::Discovered).unwrap(),
        r#""discovered""#
    );
    assert_eq!(
        serde_json::to_string(&SessionSource::Resumed).unwrap(),
        r#""resumed""#
    );
}

#[test]
fn session_with_optional_none_fields_omits_them() {
    let mut s = sample_session();
    s.tmux_session = None;
    s.tmux_pane_id = None;
    s.current_task = None;
    s.model = None;
    s.context_used_tokens = None;
    s.context_total_tokens = None;
    s.cost_usd = None;
    s.last_event_at = None;
    s.finished_at = None;
    let json = serde_json::to_value(&s).unwrap();
    assert!(
        json.get("tmux_session")
            .is_none_or(serde_json::Value::is_null)
    );
    assert!(
        json.get("tmux_pane_id")
            .is_none_or(serde_json::Value::is_null)
    );
}
