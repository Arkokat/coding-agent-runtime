# E2E smoke test

## Goal

A single script that exercises the full stack end-to-end:
1. Build `agentd` (debug profile)
2. Start the daemon in detach mode
3. Wait for the control UDS to be ready
4. Open an `opencode` session in a fresh tmux pane via `opencode run "echo hello"` (non-interactive, runs once and exits)
5. Wait for the opencode plugin to discover the pane and emit `session.discover`
6. Assert: `state.snapshot` (via the daemon's UDS) returns at least one session with `agent_type=opencode`
7. Wait for `opencode` to exit (the test runs a single command)
8. Assert: the session transitions to `finished`
9. Clean up: stop the daemon, kill the tmux session

## What to build

`scripts/e2e-smoke.sh` (new file, executable, bash). Designed to be run by hand (not as a CI test — CI sandboxes block opencode / tmux / network).

The script:
- Uses `set -euo pipefail`
- Builds `agentd` if not already built (`cargo build -p agentd`)
- Sets `XDG_RUNTIME_DIR`, `XDG_DATA_HOME`, etc. to a clean temp dir (so the test doesn't pollute the user's real daemon)
- Spawns the daemon via `target/debug/agentd daemon start --detach`
- Polls the control UDS for up to 5s
- Creates a tmux session: `tmux new-session -d -s agentd-e2e -x 200 -y 50 "opencode run 'say hello and exit'"`
- Waits up to 15s polling `state.snapshot` (via a small Rust helper OR via direct SQLite read) for a session whose `agent_type=opencode` AND `status=finished`
- Asserts the session was discovered
- Tears down

The Rust helper (`crates/agentd/tests/bin/e2e_smoke_client.rs` or similar):
- Connects to the daemon's UDS via `ControlClient::connect`
- Calls `state.snapshot` and prints the result as JSON
- Used by the bash script for the polling loop

OR the script can read the SQLite DB directly:
```bash
sqlite3 "$XDG_DATA_HOME/agentd/state.db" "SELECT id, agent_type, status FROM sessions WHERE agent_type='opencode' ORDER BY created_at DESC LIMIT 1;"
```

The SQLite approach is simpler and avoids writing Rust. Recommend that.

## Plan

1. **chore: add scripts/e2e-smoke.sh** — the bash script + SQLite-based assertions. ~80-100 lines.
2. **docs: add scripts/README.md** — explains how to run the smoke test. ~30 lines.

(2 tasks, both small. No plan, no spec — just a script + docs.)

## Constraints

- Bash (POSIX-compatible where possible, but bashisms OK since the project uses bash elsewhere)
- Must clean up on failure (set trap on EXIT for tmux + daemon)
- Must not pollute the user's real `~/.local/share/agentd/`, `~/.cache/agentd/`, etc. (use tempdir XDG vars)
- Must not require `opencode` to be installed if the test is checking for its absence — but the test IS the integration with opencode, so it requires `opencode` on PATH. Document this in `scripts/README.md`.

## File Structure

**New files:**
- `scripts/e2e-smoke.sh` (executable, bash)
- `scripts/README.md`
