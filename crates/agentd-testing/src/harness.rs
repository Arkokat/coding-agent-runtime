use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Per-test isolated environment. Owns a temp dir and exposes the
/// XDG-style layout that `agentd` would use in production.
///
/// On drop, the temp dir and all its contents are removed.
pub struct Harness {
    root: TempDir,
}

impl Harness {
    /// Create a new harness with a fresh temp dir.
    /// Creates the standard subdirs: `runtime/`, `state/`, `config/`.
    pub fn new() -> std::io::Result<Self> {
        let root = tempfile::Builder::new().prefix("agentd-test-").tempdir()?;
        std::fs::create_dir_all(root.path().join("runtime"))?;
        std::fs::create_dir_all(root.path().join("state"))?;
        std::fs::create_dir_all(root.path().join("config"))?;
        Ok(Self { root })
    }

    /// Return the root temp dir.
    pub fn temp_dir(&self) -> &Path {
        self.root.path()
    }

    /// `runtime/` subdir (sockets, locks).
    pub fn runtime_dir(&self) -> PathBuf {
        self.root.path().join("runtime")
    }

    /// `state/` subdir (`SQLite`, logs).
    pub fn state_dir(&self) -> PathBuf {
        self.root.path().join("state")
    }

    /// `config/` subdir (`config.toml`, `plugins.toml`).
    pub fn config_dir(&self) -> PathBuf {
        self.root.path().join("config")
    }

    /// Path where the control UDS will live.
    pub fn control_socket_path(&self) -> PathBuf {
        self.runtime_dir().join("control.sock")
    }

    /// Path where the `SQLite` state DB will live.
    pub fn state_db_path(&self) -> PathBuf {
        self.state_dir().join("state.db")
    }
}
