#![allow(clippy::expect_used)] // tests use .expect("reason") per project convention

use agentd_testing::http_mock::Scenario;

#[test]
fn parses_scenario_with_single_response() {
    let toml = r#"
name = "greeting"

[[step]]
[step.request]
method = "POST"
path = "/v1/chat/completions"

[step.response]
status = 200
body = '{"choices":[{"message":{"role":"assistant","content":"hi"}}]}'
content_type = "application/json"
"#;
    let s: Scenario = toml::from_str(toml).expect("parse");
    assert_eq!(s.name, "greeting");
    assert_eq!(s.steps.len(), 1);
    assert_eq!(s.steps[0].request.method, "POST");
    assert_eq!(s.steps[0].request.path, "/v1/chat/completions");
    assert_eq!(s.steps[0].response.status, 200);
    assert!(s.steps[0].response.body.contains("assistant"));
    assert_eq!(s.steps[0].response.content_type, "application/json");
}

#[test]
fn parses_scenario_with_body_hash_match() {
    let toml = r#"
name = "edit_file"

[[step]]
[step.request]
method = "POST"
path = "/v1/messages"
body_hash = "sha256:abc123"

[step.response]
status = 200
body = '{"content":[],"stop_reason":"end_turn"}'
"#;
    let s: Scenario = toml::from_str(toml).expect("parse");
    assert_eq!(s.steps[0].request.body_hash.as_deref(), Some("sha256:abc123"));
}
