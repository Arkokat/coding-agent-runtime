#![allow(clippy::expect_used)]

//! End-to-end tests for `agent_plugin_opencode::discover`.
//!
//! These tests inject a fake `tmux` shell script in a temp dir and pass
//! that path explicitly to [`discover_with_tmux`], avoiding any
//! global `PATH` mutation.

use std::path::PathBuf;

use agent_plugin_opencode::discovery::discover_with_tmux;

fn write_fake_tmux(dir: &std::path::Path, script_body: &str) -> PathBuf {
    let path = dir.join("tmux");
    std::fs::write(&path, script_body).expect("write fake tmux");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
    }
    path
}

#[tokio::test]
async fn discover_returns_empty_when_fake_tmux_reports_no_opencode() {
    let dir = tempfile::tempdir().expect("tempdir");
    // The fake `tmux` claims a pane exists with bogus PIDs. `ps -p 1` (and
    // friends) on a developer machine will return non-zero or report a
    // comm that is not "opencode", so the discovery result must be empty.
    let script = "#!/bin/sh\necho \"dev %0 1 /tmp/proj\"\necho \"dev %1 2 /tmp/other\"\n";
    let fake = write_fake_tmux(dir.path(), script);

    let panes = discover_with_tmux(&fake).await.expect("discovery ok");
    assert!(
        panes.is_empty(),
        "expected no opencode panes, got {panes:?}"
    );
}

#[tokio::test]
async fn discover_returns_empty_when_tmux_binary_missing() {
    // Point at a path that does not exist. The function should map
    // `NotFound` to `Ok(vec![])` rather than propagating an error.
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does-not-exist");
    let panes = discover_with_tmux(&missing).await.expect("discovery ok");
    assert!(panes.is_empty());
}
