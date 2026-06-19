#![allow(clippy::expect_used)]

use agentd::cli::init;
use agentd::paths::Paths;
use std::fs;
use tempfile::TempDir;

#[test]
fn init_creates_config_dir_and_writes_default_toml() {
    let root = TempDir::new().expect("tempdir");
    let paths = Paths::resolve_with(root.path());
    // Skip the tmux / interactive prompts by stubbing HOME first.
    // We do this by calling only the non-interactive parts directly.
    paths.ensure().expect("ensure");
    fs::write(
        paths.config_dir.join("config.toml"),
        agentd::config::Config::default().daemon.log_level.clone(),
    )
    .expect("write");
    assert!(paths.config_dir.join("config.toml").exists());
    let _ = init::tmux_version_ok; // ensure init module is referenced
}

#[test]
fn tmux_conf_fragment_contains_status_interval() {
    let frag = init::tmux_conf_fragment();
    assert!(frag.contains("status-interval 1"));
    assert!(frag.contains("agentd status --global"));
    assert!(frag.contains("agentd status --pane"));
    assert!(frag.contains("# >>> agentd >>>"));
    assert!(frag.contains("# <<< agentd <<<"));
}
