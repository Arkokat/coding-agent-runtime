use axum::{
    Router,
    body::Body,
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response as AxumResponse},
    routing::any,
};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

// Re-export scenario types so callers can `use agentd_testing::http_mock::*`.
pub use crate::scenario::{RequestMatch, Response, Scenario, ScenarioStep};

/// Compute the `sha256:<hex>` body hash used by the scenario matcher.
///
/// Exposed for testing. Format is `sha256:` followed by 64 lowercase
/// hex characters (256 bits), RFC 4648 base16.
pub fn hash_body(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("sha256:{digest:x}")
}

/// Counter for the per-call port assignment. Per-process monotonic; mixed
/// with the PID offset below so parallel `nextest` test processes (one
/// per test binary) don't collide on the same starting port.
static NEXT_TEST_PORT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Fixed TCP bind address used by `HttpMock::start` and the fixture tests.
///
/// Pinned to a specific range (instead of `127.0.0.1:0` OS-pick) so the host's
/// sandbox can allow-list one concrete range. Default is the 85-port window
/// `127.0.0.1:31415..=31499`; each call returns the next port in that range.
///
/// Two mechanisms keep parallel tests from colliding on `AddrInUse`:
///
/// 1. **PID offset**: the counter is mixed with `process::id() % 85`, so
///    different `nextest` test binaries (each in its own process) get
///    different starting ports. `cargo test` (single process per binary,
///    but many binaries) and `cargo nextest` both benefit.
/// 2. **`HttpMock::start` retry**: if the picked port is in use (e.g. from
///    a recently-dropped listener in `TIME_WAIT`), `start` tries up to 10
///    more ports from the counter before returning an error.
///
/// Override with the `AGENTD_TEST_PORT` env var to pin to a single port
/// (e.g. `AGENTD_TEST_PORT=18932 cargo test -p agentd-testing`).
///
/// The host sandbox must allow the range — pin `127.0.0.1:31415-31499`
/// (or a narrower range if you know your test count).
pub fn test_bind_addr() -> String {
    use std::sync::atomic::Ordering;
    if let Ok(p) = std::env::var("AGENTD_TEST_PORT") {
        if let Ok(port) = p.parse::<u16>() {
            return format!("127.0.0.1:{port}");
        }
    }
    let pid_offset = (std::process::id() as u32) % 85;
    let n = NEXT_TEST_PORT.fetch_add(1, Ordering::SeqCst);
    let port = 31415 + (pid_offset + n) % 85;
    format!("127.0.0.1:{port}")
}

/// Handle to a running mock server. Drop to stop, or call `stop`.
pub struct Handle {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl Handle {
    /// Base URL of the mock server.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Stop the mock server.
    pub fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            j.abort();
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            j.abort();
        }
    }
}

/// HTTP mock server. Binds to `test_bind_addr()` (default `127.0.0.1:31415`)
/// and replays scripted responses.
#[derive(Clone)]
pub struct HttpMock {
    scenarios: Arc<Mutex<Vec<Scenario>>>,
}

impl HttpMock {
    /// Construct a new mock with the given scenarios.
    pub fn new(scenarios: Vec<Scenario>) -> Self {
        Self {
            scenarios: Arc::new(Mutex::new(scenarios)),
        }
    }

    /// Start the mock server. Returns the base URL and a handle for shutdown.
    ///
    /// Retries on `AddrInUse` up to 10 times, calling `test_bind_addr` again
    /// each time. The PID-offset counter (see `test_bind_addr`) means a
    /// retry virtually always finds a free port; this is belt-and-braces
    /// for the rare case where a recently-dropped listener is still in
    /// `TIME_WAIT` on the same port.
    pub async fn start(self) -> std::io::Result<Handle> {
        let app = Router::new().fallback(any(handler)).with_state(self);

        let (listener, addr) = {
            let mut last_err: Option<std::io::Error> = None;
            let mut bound: Option<(tokio::net::TcpListener, std::net::SocketAddr)> = None;
            for _ in 0..10 {
                match TcpListener::bind(test_bind_addr()).await {
                    Ok(l) => match l.local_addr() {
                        Ok(a) => {
                            bound = Some((l, a));
                            break;
                        }
                        Err(e) => return Err(e),
                    },
                    Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                        last_err = Some(e);
                    }
                    Err(e) => return Err(e),
                }
            }
            bound.ok_or_else(|| {
                last_err.unwrap_or_else(|| {
                    std::io::Error::other("test_bind_addr: exhausted 10 retries on AddrInUse")
                })
            })?
        };

        let (tx, rx) = oneshot::channel();
        let join = tokio::spawn(async move {
            let server = axum::serve(listener, app);
            let _ = server
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });

        Ok(Handle {
            addr,
            shutdown: Some(tx),
            join: Some(join),
        })
    }
}

async fn handler(
    axum::extract::State(mock): axum::extract::State<HttpMock>,
    req: Request,
) -> AxumResponse {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    let body_hash = hash_body(&body_bytes);

    let scenarios = mock.scenarios.lock();
    for step in scenarios.iter().flat_map(|s| s.steps.iter()) {
        if matches(&step.request, &method, &path, &body_hash) {
            return build_response(&step.response);
        }
    }
    (
        StatusCode::NOT_FOUND,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        format!(
            r#"{{"error":"no scenario matched","method":"{method}","path":"{path}","body_hash":"{body_hash}"}}"#
        ),
    )
        .into_response()
}

fn matches(req: &RequestMatch, method: &str, path: &str, body_hash: &str) -> bool {
    if !req.method.is_empty() && req.method != method {
        return false;
    }
    if !req.path.is_empty() && req.path != path {
        return false;
    }
    if let Some(ref want) = req.body_hash {
        if want != body_hash {
            return false;
        }
    }
    true
}

fn build_response(resp: &Response) -> AxumResponse {
    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK);
    axum::response::Response::builder()
        .status(status)
        .header(axum::http::header::CONTENT_TYPE, resp.content_type.clone())
        .body(Body::from(resp.body.clone()))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every call to `test_bind_addr` must return a port in the
    /// sandbox-allowed range, regardless of how many times it's been
    /// called (counter wraps modulo 85).
    #[test]
    fn test_bind_addr_stays_in_sandbox_range() {
        for _ in 0..500 {
            let addr = test_bind_addr();
            let port: u16 = addr.rsplit(':').next().unwrap().parse().unwrap();
            assert!(
                (31415..=31499).contains(&port),
                "port {port} out of sandbox range (addr: {addr})"
            );
        }
    }

    /// Two consecutive `test_bind_addr` calls must return different ports.
    /// With the PID-offset counter, each `fetch_add(1)` advances the cycle
    /// by one, so the next call always lands on the next port in the cycle.
    #[test]
    fn test_bind_addr_consecutive_calls_differ() {
        let prev = test_bind_addr();
        let curr = test_bind_addr();
        assert_ne!(
            prev, curr,
            "consecutive calls returned the same port {prev}"
        );
    }
}
