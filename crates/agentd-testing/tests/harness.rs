#![allow(clippy::expect_used)] // tests use .expect("reason") per project convention

use agentd_testing::Harness;

#[test]
fn harness_creates_temp_dir() {
    let h = Harness::new().expect("harness");
    assert!(h.temp_dir().exists());
}

#[test]
fn harness_paths_use_xdg_layout() {
    let h = Harness::new().expect("harness");
    assert!(h.runtime_dir().ends_with("runtime"));
    assert!(h.state_dir().ends_with("state"));
    assert!(h.config_dir().ends_with("config"));
}

#[test]
fn harness_creates_subdirs_on_new() {
    let h = Harness::new().expect("harness");
    assert!(h.runtime_dir().exists());
    assert!(h.state_dir().exists());
    assert!(h.config_dir().exists());
}

#[test]
fn harness_creates_unique_temp_dirs() {
    let h1 = Harness::new().expect("h1");
    let h2 = Harness::new().expect("h2");
    assert_ne!(h1.temp_dir(), h2.temp_dir());
}

#[test]
fn harness_cleans_up_on_drop() {
    let path = {
        let h = Harness::new().expect("h");
        h.temp_dir().to_path_buf()
    };
    assert!(!path.exists(), "harness did not clean up: {path:?}");
}
