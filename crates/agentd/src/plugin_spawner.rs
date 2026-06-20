use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use thiserror::Error;
use tokio::process::Child;

/// Errors that can occur while spawning a plugin child process.
#[derive(Debug, Error)]
pub enum SpawnError {
    /// The binary path did not exist on disk before `Command::spawn` was called.
    #[error("plugin binary not found: {0}")]
    NotFound(PathBuf),
    /// Wrapped `std::io::Error` from `Command::spawn` (binary exists but exec failed,
    /// EACCES on a non-existent parent, etc.).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Handle to a spawned plugin child process. Drop does NOT kill the
/// child; callers use `kill_on_drop(true)` (set in the real impl) to
/// ensure teardown.
pub struct PluginHandle {
    pub name: String,
    pub child: Child,
}

/// Abstraction over plugin child-process spawning. The real impl shells
/// out to the plugin binary; the mock impl records calls for tests.
#[async_trait]
pub trait PluginSpawner: Send + Sync {
    async fn spawn(
        &self,
        name: &str,
        binary: &Path,
        control_socket: &Path,
    ) -> Result<PluginHandle, SpawnError>;
}

/// Test-only spawner. Records every call into `calls`; spawns the
/// provided binary as a real child so callers that await exit still
/// see realistic behavior.
pub struct MockPluginSpawner {
    pub calls: Arc<parking_lot::Mutex<Vec<(String, PathBuf, PathBuf)>>>,
}

impl MockPluginSpawner {
    /// Build a `MockPluginSpawner` that pushes every `(name, binary,
    /// control_socket)` call into `calls`. The `calls` vector is shared
    /// with the test so it can assert on what was spawned.
    pub fn new(calls: Arc<parking_lot::Mutex<Vec<(String, PathBuf, PathBuf)>>>) -> Self {
        Self { calls }
    }
}

#[async_trait]
impl PluginSpawner for MockPluginSpawner {
    async fn spawn(
        &self,
        name: &str,
        binary: &Path,
        control_socket: &Path,
    ) -> Result<PluginHandle, SpawnError> {
        self.calls.lock().push((
            name.to_string(),
            binary.to_path_buf(),
            control_socket.to_path_buf(),
        ));
        let child = tokio::process::Command::new(binary)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(SpawnError::Io)?;
        Ok(PluginHandle {
            name: name.to_string(),
            child,
        })
    }
}
