use crate::agent::AgentType;
use crate::status::SessionStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// How a session was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    /// Created via `agentd new` CLI.
    Cli,
    /// Discovered by plugin scan of an existing tmux pane.
    Discovered,
    /// Re-attached on daemon restart.
    Resumed,
}

/// A tracked coding-agent session.
///
/// `id` is a UUID v7 (time-ordered). `tmux_session` and `tmux_pane_id` are
/// `None` until the session is bound to a tmux pane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// UUID v7.
    pub id: Uuid,
    /// Which agent runs the session.
    pub agent_type: AgentType,
    /// Working directory (canonicalized).
    pub working_dir: String,
    /// tmux session name, if bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmux_session: Option<String>,
    /// tmux pane id (e.g. `%5`), if bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmux_pane_id: Option<String>,
    /// User-visible name. Defaults to `basename(working_dir)`. Renamable.
    pub display_name: String,
    /// Current status.
    pub status: SessionStatus,
    /// Human-readable current task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    /// Model identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Context window tokens used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_used_tokens: Option<u64>,
    /// Context window total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_total_tokens: Option<u64>,
    /// Running cost in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// How the session was created.
    pub source: SessionSource,
    /// Row creation time.
    pub created_at: DateTime<Utc>,
    /// Most recent event time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_event_at: Option<DateTime<Utc>>,
    /// Set when status transitions to `finished`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    /// Plugin-specific extras (`git_branch`, `agent_version`, etc.).
    #[serde(default)]
    pub metadata: Value,
}
