#![allow(clippy::io_other_error)]

use std::path::{Path, PathBuf};

use crate::ipc::control::ControlServer;

/// Plugin-side UDS server. Binds `$runtime_dir/plugin-<name>.sock`
/// with 0600 perms. Reuses `ControlServer` for the accept loop.
pub struct PluginServer {
    inner: ControlServer,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    addr: PathBuf,
}

impl PluginServer {
    pub fn bind(runtime_dir: &Path, name: &str) -> std::io::Result<Self> {
        std::fs::create_dir_all(runtime_dir)?;
        let path = runtime_dir.join(format!("plugin-{name}.sock"));
        let server = ControlServer::bind(&path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(Self {
            inner: server,
            name: name.to_string(),
            addr: path,
        })
    }

    pub fn local_addr(&self) -> &Path {
        self.inner.local_addr()
    }
}
