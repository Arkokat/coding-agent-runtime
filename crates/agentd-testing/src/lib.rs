//! agentd-testing: test harness for agentd plugins and daemon.

#![warn(missing_docs)]

mod harness;
/// HTTP mock server for plugin tests. Replays scripted responses per scenario.
pub mod http_mock;
mod scenario;
pub mod test_agent;

pub use harness::Harness;
pub use http_mock::{Handle as HttpMockHandle, HttpMock};
pub use scenario::{RequestMatch, Response, Scenario, ScenarioStep};
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
