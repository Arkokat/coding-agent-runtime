use agentd_testing::AgentEnv;
use std::process::Command;

#[test]
fn opencode_env_sets_openai_base_url() {
    let env = AgentEnv::for_opencode("http://127.0.0.1:1234");
    let mut cmd = Command::new("true");
    env.apply_to_command(&mut cmd);
    let envs: Vec<(String, String)> = cmd
        .get_envs()
        .filter_map(|(k, v)| Some((k.to_str()?.to_string(), v?.to_str()?.to_string())))
        .collect();
    assert!(
        envs.iter()
            .any(|(k, v)| k == "OPENAI_BASE_URL" && v == "http://127.0.0.1:1234")
    );
}

#[test]
fn claude_code_env_sets_anthropic_base_url() {
    let env = AgentEnv::for_claude_code("http://127.0.0.1:5678");
    let mut cmd = Command::new("true");
    env.apply_to_command(&mut cmd);
    let envs: Vec<(String, String)> = cmd
        .get_envs()
        .filter_map(|(k, v)| Some((k.to_str()?.to_string(), v?.to_str()?.to_string())))
        .collect();
    assert!(
        envs.iter()
            .any(|(k, v)| k == "ANTHROPIC_BASE_URL" && v == "http://127.0.0.1:5678")
    );
}

#[test]
fn codex_env_sets_openai_base_url() {
    let env = AgentEnv::for_codex("http://127.0.0.1:9999");
    let mut cmd = Command::new("true");
    env.apply_to_command(&mut cmd);
    let envs: Vec<(String, String)> = cmd
        .get_envs()
        .filter_map(|(k, v)| Some((k.to_str()?.to_string(), v?.to_str()?.to_string())))
        .collect();
    assert!(
        envs.iter()
            .any(|(k, v)| k == "OPENAI_BASE_URL" && v == "http://127.0.0.1:9999")
    );
}

#[test]
fn aider_env_sets_openai_api_base_via_args() {
    let env = AgentEnv::for_aider("http://127.0.0.1:7777");
    let args = env.extra_args();
    assert!(args.contains(&"--openai-api-base"));
    assert!(args.contains(&"http://127.0.0.1:7777"));
}
