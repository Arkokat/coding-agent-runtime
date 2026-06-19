#![allow(clippy::expect_used)]

use std::path::PathBuf;

use agentd_testing::{HttpMock, Scenario};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("http")
        .join("claude-code")
        .join("scenarios")
        .join("greeting.toml")
}

#[tokio::test]
async fn claude_code_greeting_fixture_replays_canned_response() {
    let body = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let scenario: Scenario = toml::from_str(&body).expect("parse fixture");
    assert_eq!(scenario.name, "greeting");
    assert_eq!(scenario.steps[0].request.path, "/v1/messages");

    let mock = HttpMock::new(vec![scenario]);
    let handle = mock.start().await.expect("start");
    let url = handle.base_url();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{url}/v1/messages"))
        .header("anthropic-version", "2023-06-01")
        .body(r#"{"model":"claude-sonnet-4-5","max_tokens":1024,"messages":[]}"#)
        .send()
        .await
        .expect("http");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["content"][0]["text"], "Hello, how can I help?");
    assert_eq!(body["model"], "claude-sonnet-4-5");

    handle.stop();
}
