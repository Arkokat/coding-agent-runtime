use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("read {0}: {1}")]
    Io(std::path::PathBuf, std::io::Error),
    #[error("parse {0}: {1}")]
    Parse(std::path::PathBuf, toml::de::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginEntry {
    /// Plugin identifier (also the UDS socket suffix and manifest key).
    pub name: String,
    /// Binary name (PATH lookup) or absolute path.
    pub binary: String,
    /// Whether the daemon should auto-spawn this plugin on boot.
    #[serde(default = "default_true")]
    pub autostart: bool,
    /// Free-form configuration passed to the plugin at startup.
    #[serde(default = "default_config_value")]
    pub config: toml::Value,
}

fn default_true() -> bool {
    true
}

fn default_config_value() -> toml::Value {
    toml::Value::Table(toml::Table::new())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PluginsManifest {
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
}

impl PluginsManifest {
    /// Load from a TOML file. Missing file → empty manifest.
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        match std::fs::read_to_string(path) {
            Ok(body) => {
                toml::from_str(&body).map_err(|e| ManifestError::Parse(path.to_path_buf(), e))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ManifestError::Io(path.to_path_buf(), e)),
        }
    }

    /// Find an entry by plugin name.
    pub fn find(&self, name: &str) -> Option<&PluginEntry> {
        self.plugins.iter().find(|p| p.name == name)
    }
}
