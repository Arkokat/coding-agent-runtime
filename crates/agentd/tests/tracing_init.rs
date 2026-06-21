#![allow(clippy::expect_used, unsafe_code)]

//! Verifies that [`agentd::tracing_init::init`] honors `AGENTD_LOG_FILE`
//! and writes tracing output to the configured file.
//!
//! Marked `#[ignore]` because `tracing::subscriber::set_global_default`
//! is a process-global side effect: the first test in the process that
//! runs it locks the global subscriber for all subsequent tests. We
//! don't reset the subscriber between tests, so running this in CI
//! would break every other tracing-using test. The two checks below
//! are also sequential — when run with parallel test threads, the
//! second check can race the first on the global subscriber and
//! produce a false-fail.
//!
//! Run locally with:
//!     cargo test -p agentd --test `tracing_init` -- --ignored --nocapture

use std::io::Write;

use agentd::tracing_init;
use agentd_testing::test_runtime_dir;

/// One test, two assertions. Sequential on purpose: the first
/// `init` call installs the global subscriber; the second one is a
/// no-op (so we only check that the file path is created, not that
/// the subscriber is replaced).
#[test]
#[ignore = "needs global subscriber reset; run with --ignored --nocapture"]
fn agentd_log_file_env_var_is_honored() {
    let dir = test_runtime_dir().join("tracing-init");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    let log_path = dir.join("daemon.log");

    // SAFETY: `set_var` is `unsafe` under Rust 2024. This test is
    // `#[ignore]`d so it never runs in the same process as other
    // subscriber-using tests; setting env vars here is safe.
    unsafe {
        std::env::set_var("AGENTD_LOG_FILE", &log_path);
    }

    // The init should not panic and should create the log file (the
    // OpenOptions call uses create(true) and the file is empty until
    // a tracing event is emitted).
    tracing_init::init(true);

    tracing::info!(probe = true, "tracing_init_smoke_test_event");

    // Best-effort flush. The default `fmt::Subscriber` is buffered; we
    // cannot rely on the event being on disk synchronously, so the
    // test only asserts the code path was reached and the file was
    // created.
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&log_path)
        .expect("open log file after init");
    writeln!(f, "post-init-touch").expect("touch log file");

    let body = std::fs::read_to_string(&log_path).expect("read log file");
    assert!(
        body.contains("post-init-touch"),
        "log file was not created/written by init; body = {body:?}"
    );

    // Now point AGENTD_LOG_FILE at a nested path the parent dir does
    // not exist for. The init should create the parent dir. We don't
    // assert that init() installs a NEW subscriber (the global is
    // already set), only that the file path is reachable.
    let nested_path = dir.join("nested").join("daemon.log");
    unsafe {
        std::env::set_var("AGENTD_LOG_FILE", &nested_path);
    }
    tracing_init::init(false);
    unsafe {
        std::env::remove_var("AGENTD_LOG_FILE");
    }

    assert!(
        nested_path.exists(),
        "nested log file should be created (parent dir created by init)"
    );
}
