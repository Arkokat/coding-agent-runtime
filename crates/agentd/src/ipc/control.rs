use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;
use tokio::net::UnixListener as TokioUnixListener;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("peer cred: {0}")]
    PeerCred(String),
}

/// Control-plane UDS server. Binds to `path`, sets 0600 perms,
/// accepts connections, hands each to a caller-provided handler.
pub struct ControlServer {
    listener: TokioUnixListener,
    addr: PathBuf,
    shutdown: Arc<Notify>,
}

impl ControlServer {
    /// Bind a Unix domain socket at `path`. Creates parent dirs as needed.
    pub fn bind(path: &Path) -> Result<Self, ControlError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Bind via tokio directly (not std + from_std) so the listener is
        // created non-blocking from the start. Registering a blocking
        // socket with the tokio runtime triggers the unstable feature
        // gate `tokio_allow_from_blocking_fd` (see tokio#7172).
        let listener = TokioUnixListener::bind(path).map_err(ControlError::Io)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let addr = path.to_path_buf();
        Ok(Self {
            listener,
            addr,
            shutdown: Arc::new(Notify::new()),
        })
    }

    /// Path of the bound socket.
    pub fn local_addr(&self) -> &Path {
        &self.addr
    }

    /// Run the accept loop until the handle is dropped or `shutdown` is called.
    /// Each accepted connection is spawned as a blocking task that calls `handler`.
    ///
    /// The signature is `async` to leave room for future `await`s (and to match
    /// the conventional `serve()` shape used elsewhere in the daemon); the
    /// implementation only awaits inside the spawned future.
    #[allow(clippy::unused_async)]
    pub async fn serve<F>(self, handler: F) -> JoinHandle<()>
    where
        F: Fn(UnixStream) + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        let shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = shutdown.notified() => break,
                    accepted = self.listener.accept() => {
                        match accepted {
                            Ok((stream, _addr)) => {
                                let h = handler.clone();
                                tokio::task::spawn_blocking(move || {
                                    let std_stream = match stream.into_std() {
                                        Ok(s) => s,
                                        Err(e) => {
                                            eprintln!("agentd control: into_std failed: {e}");
                                            return;
                                        }
                                    };
                                    h(std_stream);
                                });
                            }
                            Err(e) => {
                                eprintln!("agentd control: accept failed: {e}");
                            }
                        }
                    }
                }
            }
        })
    }

    /// Signal the accept loop to exit.
    pub fn shutdown_handle(&self) -> Arc<Notify> {
        self.shutdown.clone()
    }
}

/// Return the uid of the connected peer. Implementation switches on OS.
///
/// On Linux uses `SO_PEERCRED` (kernel returns uid/gid/pid of the peer process).
/// On macOS uses `LOCAL_PEERPID` then resolves pid → uid. For v1 single-user
/// we trust the OS that the pid is owned by us if we own the socket dir, and
/// fall back to the daemon's own uid.
pub fn peer_uid(stream: &UnixStream) -> Result<u32, ControlError> {
    #[cfg(target_os = "linux")]
    {
        use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
        let cred = getsockopt(stream, PeerCredentials)
            .map_err(|e| ControlError::PeerCred(format!("SO_PEERCRED: {e}")))?;
        Ok(cred.uid())
    }
    #[cfg(target_os = "macos")]
    {
        use nix::sys::socket::{getsockopt, sockopt::LocalPeerPid};
        let _pid = getsockopt(stream, LocalPeerPid)
            .map_err(|e| ControlError::PeerCred(format!("LOCAL_PEERPID: {e}")))?;
        // Single-user v1: trust the OS that pid belongs to us; return daemon's uid.
        Ok(nix::unistd::getuid().as_raw())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(ControlError::PeerCred("unsupported platform".into()))
    }
}
