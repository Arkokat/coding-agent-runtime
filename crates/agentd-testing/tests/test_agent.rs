#![allow(clippy::expect_used)] // tests use .expect("reason") per project convention
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::match_wildcard_for_single_variants)]

use agentd_testing::test_agent::{Script, ScriptAction};

#[test]
fn parses_single_emit_action() {
    let toml = r#"
[[action]]
after_ms = 100
emit = "session.started"
"#;
    let script: Script = toml::from_str(toml).expect("parse");
    assert_eq!(script.actions.len(), 1);
    match &script.actions[0] {
        ScriptAction::Emit { after_ms, emit } => {
            assert_eq!(*after_ms, 100);
            assert_eq!(emit, "session.started");
        }
        other => panic!("expected Emit, got {other:?}"),
    }
}

#[test]
fn parses_exit_action() {
    let toml = r#"
[[action]]
exit = true
"#;
    let script: Script = toml::from_str(toml).expect("parse");
    assert_eq!(script.actions.len(), 1);
    assert!(matches!(script.actions[0], ScriptAction::Exit));
}

#[test]
fn parses_mixed_actions() {
    let toml = r#"
[[action]]
after_ms = 50
emit = "session.status_changed"

[[action]]
after_ms = 200
emit = "session.task_changed"

[[action]]
exit = true
"#;
    let script: Script = toml::from_str(toml).expect("parse");
    assert_eq!(script.actions.len(), 3);
}
