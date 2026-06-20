use std::path::{Path, PathBuf};
use std::{fs, io};

/// All on-disk locations agentd uses, derived from XDG base directories.
///
/// In production, `Paths::resolve()` reads `XDG_CONFIG_HOME`,
/// `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_RUNTIME_DIR` (with the
/// usual `$HOME/.config` / `~/.local/share` / `~/.cache` /
/// `/run/user/<uid>` defaults). In tests, `Paths::resolve_with(root)`
/// pins everything under a temp dir.
#[derive(Debug, Clone)]
pub struct Paths {
    /// `~/.config/agentd/`
    pub config_dir: PathBuf,
    /// `~/.local/share/agentd/`
    pub state_dir: PathBuf,
    /// `~/.cache/agentd/`
    pub cache_dir: PathBuf,
    /// `$XDG_RUNTIME_DIR/agentd/`
    pub runtime_dir: PathBuf,
    /// `state_dir/state.db`
    pub state_db_path: PathBuf,
    /// `runtime_dir/control.sock`
    pub control_socket_path: PathBuf,
    /// `runtime_dir/daemon.lock` (legacy alias; prefer `daemon_lock_path`)
    pub lock_path: PathBuf,
    /// `runtime_dir/daemon.lock` — flock held while a daemon is running
    pub daemon_lock_path: PathBuf,
    /// `state_dir/logs`
    pub log_dir: PathBuf,
    /// `state_dir/plugins` (downloaded plugin binaries)
    pub plugins_dir: PathBuf,
}

impl Paths {
    /// Resolve all paths from the current process environment.
    pub fn resolve() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let cfg = std::env::var_os("XDG_CONFIG_HOME").map_or_else(
            || {
                home.as_ref()
                    .map_or_else(|| PathBuf::from(".config"), |h| h.join(".config"))
            },
            PathBuf::from,
        );
        let data = std::env::var_os("XDG_DATA_HOME").map_or_else(
            || {
                home.as_ref()
                    .map_or_else(|| PathBuf::from(".local/share"), |h| h.join(".local/share"))
            },
            PathBuf::from,
        );
        let cache = std::env::var_os("XDG_CACHE_HOME").map_or_else(
            || {
                home.as_ref()
                    .map_or_else(|| PathBuf::from(".cache"), |h| h.join(".cache"))
            },
            PathBuf::from,
        );
        let runtime =
            std::env::var_os("XDG_RUNTIME_DIR").map_or_else(|| data.join("runtime"), PathBuf::from);

        Self::from_base(
            &cfg.join("agentd"),
            &data.join("agentd"),
            &cache.join("agentd"),
            &runtime.join("agentd"),
        )
    }

    /// Resolve all paths under a single root. Used by tests and by the
    /// daemon when launched with `AGENTD_ROOT` for sandboxing.
    pub fn resolve_with(root: &Path) -> Self {
        Self::from_base(
            &root.join("config"),
            &root.join("state"),
            &root.join("cache"),
            &root.join("runtime"),
        )
    }

    fn from_base(cfg: &Path, data: &Path, cache: &Path, runtime: &Path) -> Self {
        let config_dir = cfg.to_path_buf();
        let state_dir = data.to_path_buf();
        let cache_dir = cache.to_path_buf();
        let runtime_dir = runtime.to_path_buf();
        Self {
            control_socket_path: runtime_dir.join("control.sock"),
            lock_path: runtime_dir.join("daemon.lock"),
            daemon_lock_path: runtime_dir.join("daemon.lock"),
            state_db_path: state_dir.join("state.db"),
            log_dir: state_dir.join("logs"),
            plugins_dir: state_dir.join("plugins"),
            config_dir,
            state_dir,
            cache_dir,
            runtime_dir,
        }
    }

    /// Per-plugin UDS path: `$XDG_RUNTIME_DIR/agentd/plugin-<name>.sock`.
    pub fn plugin_socket_path(&self, name: &str) -> PathBuf {
        self.runtime_dir.join(format!("plugin-{name}.sock"))
    }

    /// `runtime_dir/daemon.pid` — PID file for the running daemon.
    pub fn daemon_pid_path(&self) -> PathBuf {
        self.runtime_dir.join("daemon.pid")
    }

    /// Create all directories (idempotent).
    pub fn ensure(&self) -> io::Result<()> {
        for d in [
            &self.config_dir,
            &self.state_dir,
            &self.cache_dir,
            &self.runtime_dir,
            &self.log_dir,
            &self.plugins_dir,
        ] {
            fs::create_dir_all(d)?;
        }
        Ok(())
    }
}
