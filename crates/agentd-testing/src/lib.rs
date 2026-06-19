//! agentd-testing: test harness for agentd plugins and daemon.

#![warn(missing_docs)]

mod harness;

pub use harness::Harness;

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
