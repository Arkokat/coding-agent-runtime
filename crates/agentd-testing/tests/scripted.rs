use agentd_testing::ScriptedSession;

#[test]
fn greeting_for_opencode_matches_openai_path() {
    let scenario = ScriptedSession::greeting("opencode");
    assert_eq!(scenario.name, "opencode-greeting");
    assert_eq!(scenario.steps[0].request.path, "/v1/chat/completions");
    assert_eq!(scenario.steps[0].request.method, "POST");
}

#[test]
fn greeting_for_claude_code_matches_anthropic_path() {
    let scenario = ScriptedSession::greeting("claude-code");
    assert_eq!(scenario.name, "claude-code-greeting");
    assert_eq!(scenario.steps[0].request.path, "/v1/messages");
}

#[test]
fn greeting_for_codex_matches_openai_path() {
    let scenario = ScriptedSession::greeting("codex");
    assert_eq!(scenario.steps[0].request.path, "/v1/chat/completions");
}

#[test]
fn greeting_for_aider_matches_openai_path() {
    let scenario = ScriptedSession::greeting("aider");
    assert_eq!(scenario.steps[0].request.path, "/v1/chat/completions");
}

#[test]
fn unknown_agent_returns_default_openai_greeting() {
    let scenario = ScriptedSession::greeting("unknown");
    assert_eq!(scenario.steps[0].request.path, "/v1/chat/completions");
}

#[test]
fn all_scenarios_respond_200() {
    for agent in ["opencode", "claude-code", "codex", "aider"] {
        let scenario = ScriptedSession::greeting(agent);
        for step in &scenario.steps {
            assert_eq!(step.response.status, 200, "agent {agent} non-200 step");
        }
    }
}
