#![allow(clippy::expect_used)] // tests use .expect("reason") per project convention

use agentd_testing::{HttpMock, RequestMatch, Response, Scenario, ScenarioStep};

fn greeting_scenario() -> Scenario {
    Scenario {
        name: "greeting".into(),
        steps: vec![ScenarioStep {
            request: RequestMatch {
                method: "POST".into(),
                path: "/v1/chat/completions".into(),
                body_hash: None,
            },
            response: Response {
                status: 200,
                body: r#"{"choices":[{"message":{"role":"assistant","content":"hi"}}]}"#.into(),
                content_type: "application/json".into(),
            },
        }],
    }
}

#[tokio::test]
async fn mock_returns_matching_response() {
    let mock = HttpMock::new(vec![greeting_scenario()]);
    let handle = mock.start().await.expect("start");
    let url = handle.base_url();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{url}/v1/chat/completions"))
        .body(r#"{"model":"x","messages":[]}"#)
        .send()
        .await
        .expect("http");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["choices"][0]["message"]["content"], "hi");

    handle.stop();
}

#[tokio::test]
async fn mock_returns_404_when_no_scenario_matches() {
    let mock = HttpMock::new(vec![greeting_scenario()]);
    let handle = mock.start().await.expect("start");
    let url = handle.base_url();

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{url}/v1/whatever"))
        .send()
        .await
        .expect("http");
    assert_eq!(resp.status(), 404);

    handle.stop();
}
