#![allow(clippy::expect_used)]

use tempfile::TempDir;

#[test]
fn empty_manifest_parses() {
    let body = "# no plugins yet\n";
    let m: agentd::plugins_manifest::PluginsManifest = toml::from_str(body).expect("parse");
    assert!(m.plugins.is_empty());
}

#[test]
fn manifest_with_opencode() {
    let body = r#"
[[plugins]]
name = "opencode"
binary = "agentd-plugin-opencode"
autostart = true
config = { model = "claude-sonnet-4-5" }
"#;
    let m: agentd::plugins_manifest::PluginsManifest = toml::from_str(body).expect("parse");
    assert_eq!(m.plugins.len(), 1);
    assert_eq!(m.plugins[0].name, "opencode");
    assert!(m.plugins[0].autostart);
    assert_eq!(m.plugins[0].config["model"], "claude-sonnet-4-5".into());
}

#[test]
fn manifest_default_is_empty_list() {
    let m = agentd::plugins_manifest::PluginsManifest::default();
    assert!(m.plugins.is_empty());
}

#[test]
fn manifest_load_returns_default_when_missing() {
    let dir = TempDir::new().expect("tempdir");
    let p = dir.path().join("nope.toml");
    let m = agentd::plugins_manifest::PluginsManifest::load(&p).expect("load");
    assert!(m.plugins.is_empty());
}
