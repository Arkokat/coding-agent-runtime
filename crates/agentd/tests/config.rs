#![allow(clippy::expect_used)]

use tempfile::TempDir;

#[test]
fn default_config_has_expected_values() {
    let c = agentd::config::Config::default();
    assert_eq!(c.daemon.scan_interval_secs, 5);
    assert_eq!(c.daemon.status_interval_secs, 1);
    assert_eq!(c.ui.default_agent, "opencode");
    assert!(!c.experimental.e2e_live_tests);
}

#[test]
fn config_loads_from_toml() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
[daemon]
scan_interval_secs = 10
status_interval_secs = 2
log_level = "debug"

[ui]
default_agent = "claude-code"
color = "never"

[experimental]
e2e_live_tests = true
"#,
    )
    .expect("write");

    let c = agentd::config::Config::load(&path).expect("load");
    assert_eq!(c.daemon.scan_interval_secs, 10);
    assert_eq!(c.daemon.status_interval_secs, 2);
    assert_eq!(c.daemon.log_level, "debug");
    assert_eq!(c.ui.default_agent, "claude-code");
    assert!(c.experimental.e2e_live_tests);
}

#[test]
fn config_load_returns_default_when_file_missing() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("does-not-exist.toml");
    let c = agentd::config::Config::load(&path).expect("load missing returns default");
    assert_eq!(c.daemon.scan_interval_secs, 5);
}

#[test]
fn config_load_rejects_invalid_toml() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "this is = not [valid toml").expect("write");
    let r = agentd::config::Config::load(&path);
    assert!(r.is_err());
}
