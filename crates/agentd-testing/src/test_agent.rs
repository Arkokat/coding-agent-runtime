//! Scripted test-agent session types.
//!
//! The `test-agent` binary reads a [`Script`] (TOML) and emits the
//! corresponding events to stdout as NDJSON. Used by plugin tests to
//! simulate agent output deterministically without hitting any real
//! provider.

use serde::{Deserialize, Deserializer, Serialize};

/// Accept any input and return `()`. Used for the `Exit` unit variant so
/// that a TOML action like `exit = true` deserializes to `ScriptAction::Exit`
/// (preserving the unit-variant pattern that callers match against).
#[allow(clippy::unnecessary_wraps)] // signature mandated by `deserialize_with`
fn deserialize_anything<'de, D: Deserializer<'de>>(_d: D) -> Result<(), D::Error> {
    Ok(())
}

/// A scripted test-agent session. The test-agent binary reads this and
/// emits the corresponding events to stdout.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Script {
    /// Ordered list of actions to perform.
    #[serde(default, rename = "action")]
    pub actions: Vec<ScriptAction>,
}

/// One action in a script.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScriptAction {
    /// Emit an event after a delay.
    Emit {
        /// Delay in milliseconds before emitting.
        after_ms: u64,
        /// Event name (e.g. `session.started`, `session.status_changed`).
        emit: String,
    },
    /// Exit the test agent.
    #[serde(deserialize_with = "deserialize_anything")]
    Exit,
}
