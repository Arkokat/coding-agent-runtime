use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response as AxumResponse},
    routing::any,
    Router,
};
use parking_lot::Mutex;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

// Re-export scenario types so callers can `use agentd_testing::http_mock::*`.
pub use crate::scenario::{RequestMatch, Response, Scenario, ScenarioStep};

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

/// HTTP mock server. Binds to `127.0.0.1:0` and replays scripted responses.
#[derive(Clone)]
pub struct HttpMock {
    scenarios: Arc<Mutex<Vec<Scenario>>>,
}

impl HttpMock {
    /// Construct a new mock with the given scenarios.
    pub fn new(scenarios: Vec<Scenario>) -> Self {
        Self { scenarios: Arc::new(Mutex::new(scenarios)) }
    }

    /// Start the mock server. Returns the base URL and a handle for shutdown.
    pub async fn start(self) -> std::io::Result<Handle> {
        let app = Router::new().fallback(any(handler)).with_state(self);

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        let (tx, rx) = oneshot::channel();
        let join = tokio::spawn(async move {
            let server = axum::serve(listener, app);
            let _ = server.with_graceful_shutdown(async move {
                let _ = rx.await;
            }).await;
        });

        Ok(Handle { addr, shutdown: Some(tx), join: Some(join) })
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
    let body_hash = format!("sha256:{:x}", hash_short(&body_bytes));

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

fn hash_short(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
