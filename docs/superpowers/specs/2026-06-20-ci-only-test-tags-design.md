# CI-only test tags

## Problem

Three tests fail on developer machines whose sandbox blocks AF_UNIX `connect`:

- `crates/agentd/tests/control_client.rs::client_sends_request_and_reads_response`
- `crates/agentd/tests/control_server.rs::peer_uid_returns_current_uid_for_local_peer`
- `crates/agentd/tests/control_server.rs::server_accepts_connections_and_invokes_handler`

Without a way to skip these locally while still running them in CI, developers either carry host-specific sandbox rules (port range, UDS path) or live with red local test output. Both slow down iteration.

## Design

Mark environment-dependent tests with `#[ignore = "..."]` and configure CI to run ignored tests by default. The three UDS tests are the only known offenders; future tests in the same shape (need AF_UNIX or another host feature some sandboxes block) follow the same pattern.

### Convention

`#[ignore = "needs <feature> (some local sandboxes block it)"]` on a `#[test]` or `#[tokio::test]` function. The reason is for humans, not the runner. The `needs <feature>` prefix lets `cargo nextest list --ignored` group them by feature.

### Local

`cargo nextest run --workspace` (no flag) — ignored tests skipped.

### CI

`cargo nextest run --run-ignored all --workspace` — ignored tests run. The `--run-ignored all` flag runs *all* ignored tests, not just one per binary. (`--run-ignored` alone only runs the first ignored test per process; the `all` keyword is required.)

The `xtask` `ci` subcommand is updated to forward `--run-ignored all`.

### Sandbox

Once this lands, the host-specific rules (TCP `127.0.0.1:31415-31499`, dir `/tmp/agentd/test-uds/`) can be removed from the local dev sandbox. The `test_runtime_dir` / `test_bind_addr` defaults stay as-is for any future sandboxed environment.

## Why not `#[cfg(target_os = ...)]`

Platform-gating the tests would make them literally absent on macOS, hiding UDS code-path regressions from every local dev on the platform. `#[ignore]` is honest: the test exists, it runs in CI, local devs can opt in with `cargo nextest run --run-ignored` if their sandbox allows.

## Scope

- 3 tests tagged
- 1 line in `xtask/src/cmd.rs` (or equivalent) to add `--run-ignored all`
- 1 paragraph in `AGENTS.md` documenting the convention
- No changes to `test_runtime_dir` / `test_bind_addr` / the http_mock port pin

## Out of scope

- Removing the host-sandbox rules (manual, on each dev machine)
- Re-pinning to `127.0.0.1:0` (keeps the safety belt)
- Auto-detecting sandbox support at test time (YAGNI)
