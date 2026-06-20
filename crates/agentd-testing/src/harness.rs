#![allow(clippy::expect_used, clippy::map_unwrap_or)]

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

/// Project-stable directory for UDS sockets in tests.
///
/// Tests that bind Unix domain sockets (e.g. `ControlServer::bind`) should
/// put their sockets under this directory instead of `std::env::temp_dir()`
/// so the host sandbox can allow-list one specific path rather than
/// `/tmp/*` globally.
///
/// Default: `/tmp/agentd/test-uds/`. Override with the
/// `AGENTD_TEST_RUNTIME_DIR` env var if you prefer a different location.
///
/// **Path length constraint**: UDS paths are capped at ~108 bytes (`SUN_LEN`).
/// The default path is well under the limit; if you override, pick a path
/// whose absolute length fits.
///
/// The directory is auto-created if missing. Callers are responsible for
/// cleaning up individual socket files; the directory itself persists
/// across test runs (but `/tmp` may be cleared on reboot).
///
/// # Panics
///
/// Panics if the runtime directory cannot be created.
pub fn test_runtime_dir() -> PathBuf {
    let dir = if let Some(custom) = std::env::var_os("AGENTD_TEST_RUNTIME_DIR") {
        PathBuf::from(custom)
    } else {
        PathBuf::from("/tmp/agentd/test-uds")
    };
    std::fs::create_dir_all(&dir).expect("create test_runtime_dir");
    dir
}

/// Return a unique socket path under `test_runtime_dir()` for use in a
/// test. Combines a hash of the caller-supplied label with an atomic
/// counter so concurrent test functions don't collide.
///
/// The file is NOT created or cleaned up by this function; the caller
/// binds via `UnixListener::bind` (which creates the file) and is
/// expected to `std::fs::remove_file` after the test.
///
/// **Filename format**: `{label_hash:x}-{counter}.sock` (e.g. `a1b2c3d4-0.sock`).
/// The label is hashed rather than embedded verbatim because UDS paths
/// are capped at ~108 bytes (`SUN_LEN`) and the worktree path is already
/// long. The hash is `DefaultHasher` — collisions across labels are
/// astronomically unlikely and only a problem if you somehow have more
/// than 2^32 tests with different labels; the atomic counter prevents
/// intra-process collisions regardless.
pub fn test_socket_path(label: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    label.hash(&mut hasher);
    let label_hash = hasher.finish();
    test_runtime_dir().join(format!("{label_hash:x}-{n}.sock"))
}
