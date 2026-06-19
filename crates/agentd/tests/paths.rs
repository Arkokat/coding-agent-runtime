#![allow(clippy::expect_used)]

use tempfile::TempDir;

#[test]
fn resolve_with_uses_xdg_layout_under_root() {
    let root = TempDir::new().expect("tempdir");
    let paths = agentd::paths::Paths::resolve_with(root.path());
    assert_eq!(paths.config_dir, root.path().join("config"));
    assert_eq!(paths.state_dir, root.path().join("state"));
    assert_eq!(paths.cache_dir, root.path().join("cache"));
    assert_eq!(paths.runtime_dir, root.path().join("runtime"));
    assert_eq!(
        paths.control_socket_path,
        root.path().join("runtime/control.sock")
    );
    assert_eq!(paths.state_db_path, root.path().join("state/state.db"));
    assert_eq!(paths.lock_path, root.path().join("runtime/daemon.lock"));
    assert_eq!(paths.log_dir, root.path().join("state/logs"));
    assert_eq!(paths.plugins_dir, root.path().join("state/plugins"));
}

#[test]
fn resolve_with_creates_subdirs() {
    let root = TempDir::new().expect("tempdir");
    let paths = agentd::paths::Paths::resolve_with(root.path());
    paths.ensure().expect("ensure");
    for d in [
        &paths.config_dir,
        &paths.state_dir,
        &paths.cache_dir,
        &paths.runtime_dir,
        &paths.log_dir,
    ] {
        assert!(d.exists(), "missing dir: {d:?}");
    }
}
