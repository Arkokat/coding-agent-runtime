use serde::{Deserialize, Serialize};

/// A connected plugin process.
///
/// `name` is also the UDS socket suffix and the manifest key, so it must be
/// unique per daemon. `binary` is the on-disk path the plugin was launched
/// from (informational — daemon re-derives it from `plugins.toml` on restart).
/// `socket_name` is the file name inside `$XDG_RUNTIME_DIR/agentd/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plugin {
    /// Plugin identifier (also the UDS socket suffix and manifest key).
    pub name: String,
    /// Binary name (PATH lookup) or absolute path.
    pub binary: String,
    /// UDS socket file name (e.g. `opencode.sock`).
    pub socket_name: String,
}
