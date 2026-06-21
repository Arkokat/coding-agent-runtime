# Opencode integration plan

## Goal

Replace the reference opencode plugin (currently reads from stdin) with a tmux pane watcher: discovers panes running `opencode`, registers each via `session.discover`, and polls pane content for status changes (working/idle/errored), emitting events to the daemon.

## Architecture

Plugin binary `agentd-plugin-opencode` has 3 modes:
- `--watch` (default in real mode): tmux pane discovery + status polling loop
- `--stdin`: NDJSON from stdin (backward compat)
- `--mock`: scripted events (backward compat, used in tests)

New modules:
- `crates/agent-plugin-opencode/src/discovery.rs` — scan tmux for opencode panes
- `crates/agent-plugin-opencode/src/watcher.rs` — poll pane content, parse status, emit events

The watcher shells out to `tmux list-panes -a` and `tmux capture-pane` via `tokio::process::Command` (same pattern as the daemon's `RealTmux`).

## Tasks

1. **feat(plugin-opencode): tmux pane discovery** — `discovery::discover_opencode_panes() -> Vec<OpencodePane>` (session_name, pane_id, pane_pid, working_dir). For each pane, read `/proc/<pid>/comm` (Linux) or `ps -p <pid> -o comm=` (macOS) to check if it's opencode. Unit tests with a fixture /proc filesystem or recorded ps output.

2. **feat(plugin-opencode): status watcher + report loop** — `watcher::run(client, panes, interval)` that calls `session.discover` for each new pane, then every `interval` calls `tmux capture-pane -p -t <pane> -S -50`, parses the last non-empty line for status keywords, and calls `session.report_event` on status change. Also runs a 5s heartbeat loop.

3. **feat(plugin-opencode): wire watcher into main.rs as default mode** — Add `--watch` flag (default). `--stdin` and `--mock` keep their existing behavior. The new `--watch` runs `discover_opencode_panes` then `watcher::run` in a loop until `client.bye()` or process exit.

4. **chore(plugin-opencode): mock tmux for unit tests** — Add a test helper that creates a temp directory with a fake `/proc/<pid>/comm` file containing "opencode\n", and a script that fakes `tmux list-panes` + `tmux capture-pane` outputs. Use it to test the discovery + status parsing.

## Constraints

- Rust 2024, MSRV 1.85
- `cargo fmt --check` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo doc --no-deps` clean
- No `unwrap()` in non-test code
- All public APIs have `///` doc comments
- TDD: red → green → refactor → commit, one task = one commit
- Module declarations in `lib.rs` (not `main.rs`)
- Conventional Commits, subject ≤50 chars

## File Structure

**New files:**
- `crates/agent-plugin-opencode/src/discovery.rs`
- `crates/agent-plugin-opencode/src/watcher.rs`
- `crates/agent-plugin-opencode/src/lib.rs` (re-exports for tests)
- `crates/agent-plugin-opencode/tests/discovery.rs`
- `crates/agent-plugin-opencode/tests/watcher.rs`

**Modified files:**
- `crates/agent-plugin-opencode/src/main.rs` (add `--watch` mode, default)

## Status detection keywords

`crates/agent-plugin-opencode/src/watcher.rs::parse_status`:

- Working: line contains "Compiling" / "Building" / "Running" / "Loading" / "Processing"
- Errored: line contains "Error" / "error[E" (Rust) / "panic" / "failed" / "Failed"
- Idle: line ends with the opencode prompt `❯` (with optional whitespace)
- Otherwise: previous status (or "starting" if first poll)

Emit `session.report_event` only on status change. Always emit `session.task_changed` if the prompt is non-empty (the last "active task" line).

## Execution

Subagent-driven per the established pattern. 4 task dispatches + 4 reviews. Total ~3-5 hours of work for a fresh subagent on each.
