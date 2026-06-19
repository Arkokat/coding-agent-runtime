#![allow(clippy::expect_used)]

use agentd_protocol::{
    AgentType, Method, ProtocolError, ProtocolErrorWithMessage, Session, SessionSource,
    SessionStatus,
};
use chrono::{TimeZone, Utc};
use insta::assert_json_snapshot;
use uuid::Uuid;

#[test]
fn snapshot_session_status_starting() {
    assert_json_snapshot!(SessionStatus::Starting);
}

#[test]
fn snapshot_session_status_waiting_for_user() {
    assert_json_snapshot!(SessionStatus::WaitingForUser);
}

#[test]
fn snapshot_agent_type_claude_code() {
    assert_json_snapshot!(AgentType::ClaudeCode);
}

#[test]
fn snapshot_session_source_resumed() {
    assert_json_snapshot!(SessionSource::Resumed);
}

#[test]
fn snapshot_protocol_error_parse_error_with_message() {
    let e: ProtocolErrorWithMessage = ProtocolError::ParseError.with_message("bad json");
    assert_json_snapshot!(e);
}

#[test]
fn snapshot_full_session() {
    let session = Session {
        id: Uuid::parse_str("019065a1-7c9a-7def-8a1b-1234567890ab").unwrap(),
        agent_type: AgentType::Opencode,
        working_dir: "/Users/me/projects/agentd".into(),
        tmux_session: Some("agentd-019065a1".into()),
        tmux_pane_id: Some("%5".into()),
        display_name: "agentd".into(),
        status: SessionStatus::Working,
        current_task: Some("editing src/main.rs".into()),
        model: Some("claude-sonnet-4-5".into()),
        context_used_tokens: Some(42000),
        context_total_tokens: Some(200_000),
        cost_usd: Some(0.12),
        source: SessionSource::Cli,
        created_at: Utc.with_ymd_and_hms(2026, 6, 18, 12, 0, 0).unwrap(),
        last_event_at: Some(Utc.with_ymd_and_hms(2026, 6, 18, 12, 5, 30).unwrap()),
        finished_at: None,
        metadata: serde_json::json!({"git_branch": "main", "agent_version": "1.2.3"}),
    };
    assert_json_snapshot!(session);
}

#[test]
fn snapshot_method_constants_table() {
    // Snapshot of the full list of method names. If you add a method,
    // this snapshot changes on purpose.
    let all = [
        Method::STATE_SNAPSHOT,
        Method::SESSION_CREATE,
        Method::SESSION_RENAME,
        Method::SESSION_JUMP,
        Method::SESSION_KILL,
        Method::SESSION_DISMISS_ERROR,
        Method::SESSION_GET,
        Method::SESSION_EVENTS,
        Method::DAEMON_STATUS,
        Method::DAEMON_SHUTDOWN,
        Method::PLUGIN_LIST,
        Method::PLUGIN_START,
        Method::PLUGIN_STOP,
        Method::PLUGIN_INSTALL,
        Method::PLUGIN_UPDATE,
        Method::PLUGIN_REMOVE,
        Method::SUBSCRIBE,
        Method::UNSUBSCRIBE,
        Method::METRICS,
        Method::PLUGIN_HELLO,
        Method::SESSION_REPORT_EVENT,
        Method::SESSION_DISCOVER,
        Method::PLUGIN_HEARTBEAT,
        Method::PLUGIN_BYE,
        Method::EVENT,
    ];
    assert_json_snapshot!(all);
}
