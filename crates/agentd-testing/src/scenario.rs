use serde::{Deserialize, Serialize};

/// One scenario: a named sequence of (request match, response) pairs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    /// Human-readable name, used in logs and test names.
    pub name: String,
    /// Ordered steps. First matching step wins.
    #[serde(default, rename = "step")]
    pub steps: Vec<ScenarioStep>,
}

/// One step in a scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioStep {
    /// The request to match against.
    pub request: RequestMatch,
    /// The response to return if matched.
    pub response: Response,
}

/// Match criteria for an incoming request. All specified fields must match.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestMatch {
    /// HTTP method (e.g. "POST"). Empty = any.
    #[serde(default)]
    pub method: String,
    /// Request path (e.g. "/v1/messages"). Empty = any.
    #[serde(default)]
    pub path: String,
    /// Hash of the request body. Format: `"sha256:<hex>"`. None = ignore.
    #[serde(default)]
    pub body_hash: Option<String>,
}

/// The response to return when a request matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// HTTP status code.
    pub status: u16,
    /// Response body (raw bytes as string).
    pub body: String,
    /// Content-Type header value. Defaults to "application/json".
    #[serde(default = "default_content_type")]
    pub content_type: String,
}

fn default_content_type() -> String {
    "application/json".to_string()
}
