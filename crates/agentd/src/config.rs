use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("read {0}: {1}")]
    Io(std::path::PathBuf, std::io::Error),
    #[error("parse {0}: {1}")]
    Parse(std::path::PathBuf, toml::de::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonSection {
    /// Seconds between plugin scan loops.
    pub scan_interval_secs: u64,
    /// Seconds between status line cache rebuilds.
    pub status_interval_secs: u64,
    /// Tracing filter default: trace|debug|info|warn|error.
    pub log_level: String,
}

impl Default for DaemonSection {
    fn default() -> Self {
        Self {
            scan_interval_secs: 5,
            status_interval_secs: 1,
            log_level: "info".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiSection {
    /// Agent selected by default in `agentd new` picker.
    pub default_agent: String,
    /// auto|always|never.
    pub color: String,
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            default_agent: "opencode".into(),
            color: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ExperimentalSection {
    /// Allow `LIVE_LLM_TESTS=1` to hit real LLM APIs. Off by default.
    pub e2e_live_tests: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonSection,
    #[serde(default)]
    pub ui: UiSection,
    #[serde(default)]
    pub experimental: ExperimentalSection,
}

impl Config {
    /// Load from a TOML file. Missing file → defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(body) => toml::from_str(&body).map_err(|e| ConfigError::Parse(path.to_path_buf(), e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ConfigError::Io(path.to_path_buf(), e)),
        }
    }
}
