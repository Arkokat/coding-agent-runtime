//! agentd-testing: test harness for agentd plugins and daemon.
//!
//! Provides (added in later tasks of this plan):
//! - `Harness` — temp dir, XDG layout, cleanup on drop
//! - `test_agent` binary — fixture that emits scripted events
//! - `HttpMock` — axum-based server that replays canned responses per scenario
//! - `ScriptedSession` — fluent builder for common test flows
//! - `AgentEnv` — per-agent base URL helpers

#![warn(missing_docs)]

/// Return the testing crate version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert_eq!(version(), "0.1.0");
    }
}
