use std::process::Command;

/// Knows how to point a specific coding agent at a mock HTTP server.
pub struct AgentEnv {
    /// Which agent this targets.
    pub agent: &'static str,
    /// Base URL the agent should hit.
    pub base_url: String,
}

impl AgentEnv {
    /// opencode -> `OPENAI_BASE_URL`
    pub fn for_opencode(base_url: impl Into<String>) -> Self {
        Self { agent: "opencode", base_url: base_url.into() }
    }
    /// Claude Code -> `ANTHROPIC_BASE_URL`
    pub fn for_claude_code(base_url: impl Into<String>) -> Self {
        Self { agent: "claude-code", base_url: base_url.into() }
    }
    /// Codex -> `OPENAI_BASE_URL`
    pub fn for_codex(base_url: impl Into<String>) -> Self {
        Self { agent: "codex", base_url: base_url.into() }
    }
    /// Aider -> `--openai-api-base` flag (not env var)
    pub fn for_aider(base_url: impl Into<String>) -> Self {
        Self { agent: "aider", base_url: base_url.into() }
    }

    /// Apply the env var(s) to a `Command` so the agent process points at the mock.
    ///
    /// # Panics
    ///
    /// Panics if `self.agent` is not one of the four known agents
    /// (`opencode`, `codex`, `claude-code`, `aider`).
    pub fn apply_to_command(&self, cmd: &mut Command) {
        match self.agent {
            "opencode" | "codex" => { cmd.env("OPENAI_BASE_URL", &self.base_url); }
            "claude-code" => { cmd.env("ANTHROPIC_BASE_URL", &self.base_url); }
            "aider" => { /* uses extra_args, no env var */ }
            other => panic!("unknown agent: {other}"),
        }
    }

    /// Extra CLI args to append to the agent command.
    pub fn extra_args(&self) -> Vec<&str> {
        match self.agent {
            "aider" => vec!["--openai-api-base", &self.base_url],
            _ => vec![],
        }
    }
}
