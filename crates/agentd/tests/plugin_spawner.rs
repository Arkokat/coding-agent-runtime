#![allow(clippy::expect_used)]

use agentd::plugin_spawner::{MockPluginSpawner, PluginSpawner};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn true_path() -> &'static Path {
    if cfg!(target_os = "macos") {
        Path::new("/usr/bin/true")
    } else {
        Path::new("/bin/true")
    }
}

#[tokio::test]
async fn mock_spawner_records_call_and_returns_handle() {
    let calls: Arc<Mutex<Vec<(String, PathBuf, PathBuf)>>> = Arc::new(Mutex::new(Vec::new()));
    let s = MockPluginSpawner::new(Arc::clone(&calls));
    // We can't easily fabricate a real `Child`, so the mock returns one
    // backed by a no-op binary that exits 0 immediately and a name we can match.
    // `/bin/true` on Linux, `/usr/bin/true` on macOS — both are coreutils' `true`.
    let true_bin = true_path();
    let h = s
        .spawn("opencode", true_bin, Path::new("/tmp/agentd/control.sock"))
        .await
        .expect("spawn");
    assert_eq!(h.name, "opencode");

    // Allow the child to exit.
    drop(h);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let recorded = calls.lock().clone();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].0, "opencode");
    assert_eq!(recorded[0].1, PathBuf::from(true_bin));
    assert_eq!(recorded[0].2, PathBuf::from("/tmp/agentd/control.sock"));
}
