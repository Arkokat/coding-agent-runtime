use crate::scenario::{RequestMatch, Response, Scenario, ScenarioStep};

/// Static factory methods for common per-agent test scenarios.
///
/// Each method constructs a fresh `Scenario` value; there is no
/// internal state to mutate. (For chainable scenario assembly, use
/// `Scenario` + `ScenarioStep` directly.)
pub struct ScriptedSession;

impl ScriptedSession {
    /// Greeting scenario: one POST, one response.
    pub fn greeting(agent: &str) -> Scenario {
        let (path, body) = match agent {
            "claude-code" => (
                "/v1/messages",
                r#"{"id":"msg_01","type":"message","role":"assistant","content":[{"type":"text","text":"Hello, how can I help?"}],"model":"claude-sonnet-4-5","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":8}}"#,
            ),
            _ => (
                "/v1/chat/completions",
                r#"{"id":"chatcmpl-1","object":"chat.completion","created":1700000000,"model":"claude-sonnet-4-5","choices":[{"index":0,"message":{"role":"assistant","content":"Hello, how can I help?"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":8,"total_tokens":18}}"#,
            ),
        };
        Scenario {
            name: format!("{agent}-greeting"),
            steps: vec![ScenarioStep {
                request: RequestMatch {
                    method: "POST".into(),
                    path: path.into(),
                    body_hash: None,
                },
                response: Response {
                    status: 200,
                    body: body.into(),
                    content_type: "application/json".into(),
                },
            }],
        }
    }
}
