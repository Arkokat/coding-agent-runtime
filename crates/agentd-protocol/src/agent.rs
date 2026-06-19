use serde::{Deserialize, Serialize};

/// Which coding agent a session is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentType {
    /// `opencode`
    Opencode,
    /// Claude Code (Anthropic)
    ClaudeCode,
    /// Codex (`OpenAI`)
    Codex,
    /// Aider
    Aider,
}

impl AgentType {
    /// All known agent types in declaration order.
    pub const ALL: &'static [AgentType] = &[
        AgentType::Opencode,
        AgentType::ClaudeCode,
        AgentType::Codex,
        AgentType::Aider,
    ];

    /// Canonical string id (e.g. `"claude-code"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            AgentType::Opencode => "opencode",
            AgentType::ClaudeCode => "claude-code",
            AgentType::Codex => "codex",
            AgentType::Aider => "aider",
        }
    }
}
