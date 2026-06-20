//! agentd-testing: test harness for agentd plugins and daemon.

#![warn(missing_docs)]

mod agent_env;
mod harness;
/// HTTP mock server for plugin tests. Replays scripted responses per scenario.
pub mod http_mock;
mod sample_session;
mod scenario;
mod scripted;
pub mod test_agent;

pub use agent_env::AgentEnv;
pub use harness::{Harness, test_runtime_dir, test_socket_path};
pub use http_mock::{Handle as HttpMockHandle, HttpMock, test_bind_addr};
pub use sample_session::sample_session;
pub use scenario::{RequestMatch, Response, Scenario, ScenarioStep};
pub use scripted::ScriptedSession;
pub use test_agent::{Script, ScriptAction};

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
