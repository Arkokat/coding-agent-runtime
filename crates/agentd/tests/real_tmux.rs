#![allow(clippy::expect_used)]

use agentd::tmux::{RealTmux, Tmux, tmux_version_ok, validate_pane_id, validate_session_name};

#[test]
fn validators_reject_bad_input() {
    assert!(validate_session_name("good_name-1.0").is_ok());
    assert!(validate_session_name("").is_err());
    assert!(validate_session_name("a/b").is_err());
    assert!(validate_session_name(&"x".repeat(65)).is_err());
    assert!(validate_pane_id("%1").is_ok());
    assert!(validate_pane_id("1").is_err());
    assert!(validate_pane_id("%x").is_err());
}

#[test]
#[ignore = "needs real tmux binary on PATH"]
fn real_tmux_round_trip() {
    if !tmux_version_ok() {
        eprintln!("tmux < 2.6 or not in PATH; skipping");
        return;
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let tm = RealTmux::new();
        let name = "agentd-test-rm";
        // Best-effort cleanup of any prior test residue.
        let _ = tm.kill_session(name).await;

        let pane = tm.new_session(name, "/tmp").await.expect("new_session");
        assert!(validate_pane_id(&pane).is_ok(), "pane {pane} not valid");
        assert!(tm.has_session(name).await);

        tm.kill_session(name).await.expect("kill");
        assert!(!tm.has_session(name).await);
    });
}
