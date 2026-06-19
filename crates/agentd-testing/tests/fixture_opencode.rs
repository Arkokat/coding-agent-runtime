#![allow(clippy::expect_used)]

use std::path::PathBuf;

use agentd_testing::http_mock::hash_body;
use agentd_testing::{HttpMock, Scenario};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("http")
        .join("opencode")
        .join("scenarios")
        .join("greeting.toml")
}

#[tokio::test]
async fn opencode_greeting_fixture_replays_canned_response() {
    let body = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let scenario: Scenario = toml::from_str(&body).expect("parse fixture");
    assert_eq!(scenario.name, "greeting");

    let mock = HttpMock::new(vec![scenario]);
    let handle = mock.start().await.expect("start");
    let url = handle.base_url();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{url}/v1/chat/completions"))
        .body(r#"{"model":"claude-sonnet-4-5","messages":[]}"#)
        .send()
        .await
        .expect("http");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "Hello, how can I help?"
    );
    assert_eq!(body["model"], "claude-sonnet-4-5");

    handle.stop();
}

#[tokio::test]
async fn opencode_greeting_body_hash_matches_request() {
    let body = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let scenario: Scenario = toml::from_str(&body).expect("parse fixture");
    let step = &scenario.steps[0];

    // Fixture has no body_hash set, so the handler must not filter on body hash.
    assert!(
        step.request.body_hash.is_none(),
        "fixture should not pin body hash"
    );

    // The hash of the request body should be a valid sha256:hex string
    // (regression: would be DefaultHasher format pre-fix).
    let request_body = br#"{"model":"x","messages":[]}"#;
    let h = hash_body(request_body);
    assert!(h.starts_with("sha256:"));
    assert_eq!(h.len(), 71);
}
