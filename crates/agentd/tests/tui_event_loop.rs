#![allow(clippy::expect_used)]

// Smoke test: verify `agentd::tui::run` has the expected signature
// (async fn returning `anyhow::Result<()>`). The actual TUI requires a
// running daemon + a real terminal, so we do not invoke it here — we
// only check the type at compile time.

#[test]
fn run_signature() {
    // Compile-time check: `run` is an async fn returning `anyhow::Result<()>`.
    // If `run` is missing or has the wrong signature, this won't compile.
    fn assert_async<F>(_fut: F)
    where
        F: std::future::Future<Output = anyhow::Result<()>>,
    {
    }
    assert_async(agentd::tui::run());
}
