//! agentd-protocol: JSON-RPC 2.0 types and method constants.

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod agent;
mod error;
mod methods;
mod plugin;
mod session;
mod status;
mod version;

pub use agent::AgentType;
pub use error::{ProtocolError, ProtocolErrorWithMessage};
pub use methods::Method;
pub use plugin::Plugin;
pub use session::{Session, SessionSource};
pub use status::SessionStatus;
pub use version::PROTOCOL_VERSION;

/// Return the protocol crate version string.
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

    #[test]
    fn all_statuses_returns_six_variants() {
        assert_eq!(SessionStatus::ALL.len(), 6);
    }

    #[test]
    fn only_finished_is_terminal() {
        assert!(SessionStatus::Finished.is_terminal());
        for s in SessionStatus::ALL {
            if *s != SessionStatus::Finished {
                assert!(!s.is_terminal(), "{s} should not be terminal");
            }
        }
    }

    #[test]
    fn all_agent_types_returns_four_variants() {
        assert_eq!(AgentType::ALL.len(), 4);
    }
}
