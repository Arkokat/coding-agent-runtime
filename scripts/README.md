# scripts/

Hand-run smoke test for the full agentd stack. Not for CI.

## e2e-smoke.sh

Exercises the full stack end-to-end on a developer host:

1. Builds `target/debug/agentd` if not already built.
2. Sets up a clean temp XDG tree (so the test does not pollute the
   user's real `~/.local/share/agentd`, `~/.config/agentd`, etc).
3. Starts the daemon via `target/debug/agentd daemon start --detach`.
4. Polls the control UDS at `$XDG_RUNTIME_DIR/agentd/control.sock` for up to 5s.
5. Spawns `opencode run "say hello and exit"` in a fresh tmux pane
   (session `agentd-e2e`).
6. Polls `$XDG_DATA_HOME/agentd/state.db` for a session row with
   `agent_type='opencode'` AND `status='finished'`. Timeout 30s.
7. Asserts at least one opencode session was discovered AND at least
   one reached `finished` status.
8. Stops the daemon and kills the tmux session.

### Run it

```bash
bash scripts/e2e-smoke.sh
```

or, once made executable:

```bash
./scripts/e2e-smoke.sh
```

Exit status: `0` on pass, `1` on any failure.

### Prerequisites

- `cargo` (build only)
- `sqlite3` (DB poll)
- `tmux` (spawn the opencode session)
- `opencode` (the agent under test)
- A `target/debug/agentd-plugin-opencode` reachable by the daemon
  (the script auto-builds `agentd`; the plugin must already be built
  or reachable on `$PATH` / at `./target/debug/agentd-plugin-opencode`)

### What it tests

- The daemon starts cleanly under an isolated XDG tree.
- The autostart manifest in `plugins.toml` correctly spawns the
  opencode plugin.
- The opencode plugin discovers a running `opencode` tmux pane and
  emits `session.discover` to the daemon.
- The plugin emits `session.status_changed` / `session.finished`
  events for the discovered session.
- The daemon persists those events to SQLite, and `state.snapshot`
  would show the session with `status=finished`.

### Sandbox limitations

This script is **not for CI**. It needs capabilities that some
sandboxes (notably nono with default profiles) restrict:

- `AF_UNIX` socket **bind** for the control UDS and per-plugin UDS.
- Subprocess execution (`opencode`, `agentd-plugin-opencode`).
- Read/write to a fresh temp directory.

If the daemon prints
`Error: control: io: Operation not permitted (os error 1)` on
start, the sandbox is blocking `AF_UNIX` bind. To allow it in nono,
draft a profile extension to `~/.config/nono/profile-drafts/`:

```json
{
  "extends": "opencode-rust",
  "meta": { "name": "opencode-rust-uds", "version": "1.0.0" },
  "filesystem": {
    "unix_socket_subtree_bind": ["<absolute-tmpdir-prefix>/agentd-e2e.*/"],
    "unix_socket_dir_bind": ["<absolute-tmpdir-prefix>/agentd-e2e.*/runtime/agentd/"]
  }
}
```

then `nono profile promote opencode-rust-uds` and re-launch with
`nono run --profile opencode-rust-uds -- opencode`.

(Without a specific UDS allowlist, macOS Seatbelt's `af_unix`
mediation rejects the daemon's `bind()` call even on tmpfs paths
that the filesystem layer otherwise allows.)

### Cleanup

The script registers a `trap cleanup EXIT INT TERM` that:

- Stops the daemon (`agentd daemon stop`).
- Kills the `agentd-e2e` tmux session.
- Removes the temp XDG tree.

It is safe to `Ctrl-C` partway through.
