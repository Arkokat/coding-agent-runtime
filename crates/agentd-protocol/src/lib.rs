//! agentd-protocol: JSON-RPC 2.0 types and method constants.
//!
//! No async. No I/O. Pure data. This crate is the shared vocabulary
//! between `agentd` (daemon), `agentd-testing` (harness), and all
//! `agent-plugin-*` crates.

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod version;

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
}
