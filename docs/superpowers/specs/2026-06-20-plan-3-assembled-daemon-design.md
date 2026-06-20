# Plan 3 — Assembled daemon runtime

## Goal

The agentd daemon runs as a long-lived process: spawns plugins, accepts control UDS requests, handles the full session lifecycle. CLI subcommands that today are `println!("not yet implemented")` actually work end-to-end. The skeleton that Plan 2 left behind (control UDS server, plugin UDS server, handler dispatcher, plugin supervisor, Tmux trait, `MockTmux`, `MockPluginSpawner`) is wired together.

## Scope

Everything needed for `agentd daemon start --foreground` → user runs `agentd new ./foo` → agent spawns in tmux → plugin reports events → `agentd status --global` shows the session → `agentd jump <id>` switches to the tmux window → `agentd kill <id>` cleans up — all in the foreground or detached, with double-start protection and graceful shutdown.

## Out of scope (deferred)

- TUI dashboard (§6) — Plan 4
- Interactive `agentd new --pick` fuzzy picker (§12 modal) — Plan 4
- Per-agent plugins beyond opencode (§7) — Plan 5
- Config hot-reload (§11 partially) — Plan 4
- Metrics, debug bundle, uninstall (§13/§15/§18) — Plan 6
- systemd/launchd integration (§4) — post-v1

## Design

### 1. New `daemon` module

`crates/agentd/src/daemon.rs` declares the `Daemon` struct and the boot sequence. The struct owns:

- `paths: Paths` — resolved XDG paths (config, data, state, runtime)
- `db: Db` — opened SQLite
- `bus: EventBus` — shared broadcast bus
- `tmux: Box<dyn Tmux>` — `RealTmux` in production, `MockTmux` in tests
- `supervisor: PluginSupervisor` — with real spawn wiring
- `control_server: ControlServer` — bound to control UDS
- `plugin_server: PluginServer` — bound to a UDS per plugin (see §3)
- `shutdown: AtomicBool` — set by `daemon.shutdown` RPC

`Daemon::run(self)` is the entry point: it executes the boot sequence and parks on the broadcast bus until shutdown.

### 2. Boot sequence (§4 step-by-step)

```
fn run(self) -> Result<(), DaemonError> {
    1. acquire_flock(&paths.daemon_lock_path)?           // no double-daemon
    2. mkdir_runtime(&paths)?                            // 0700
    3. Db::open(&paths.state_db_path)?; migrations::run(&db)?;
    4. tombstone_gc(&db)?                                // 30-day cleanup
    5. restart_reconfirm(&db, &supervisor)?             // see §6
    6. control_server.bind(&paths.control_socket_path)?;
    7. supervisor.autostart(&paths)?                     // spawns each plugin
    8. serve_until_shutdown(...)                         // idle loop
}
```

Steps 1–5 happen before any UDS is bound, so a partially-bootstrapped daemon is invisible to clients. Step 6 binds the control UDS — the daemon is now reachable. Step 7 spawns plugins; failure of an individual plugin logs a warning but does not abort the boot. Step 8 blocks on `bus` + `shutdown`.

### 3. Per-plugin UDS

Plan 2 already created `PluginServer` in `crates/agentd/src/ipc/plugin.rs` (mirrors `ControlServer`). Plan 3 wires the per-plugin socket path: `$XDG_RUNTIME_DIR/agentd/plugin-<name>.sock`, 0600 perms, owned by the user.

The supervisor binds the plugin UDS first, then spawns the plugin child process. The plugin child connects to the UDS as the client. The `plugin.hello` handshake records `last_connected_at` in the `plugins` table.

### 4. PluginSpawner trait

New trait in `crates/agentd/src/plugin_supervisor.rs`:

```rust
#[async_trait]
pub trait PluginSpawner: Send + Sync {
    async fn spawn(&self, name: &str, binary: &Path) -> Result<Child, SpawnError>;
}
```

Two impls:

- `RealPluginSpawner` (default) — `tokio::process::Command::new(binary).args(["--control-socket", socket]).spawn()` with `kill_on_drop(true)`. Stdout/stderr piped to `tracing`.
- `MockPluginSpawner` (tests) — records the call, returns a fake `Child` that immediately "exits" (so heartbeat timeout is testable). The tests can also drive the supervisor's spawn loop with a list of pre-recorded children.

### 5. Plugin heartbeat + restart

A `PluginHeartbeat` task per plugin, spawned in the supervisor, owns:

- `last_heartbeat: Mutex<Instant>` — updated by the heartbeat handler
- `events_total: AtomicU64` — incremented by the heartbeat handler
- `invalid_total: AtomicU64` — same

A second supervisor task wakes every 5s. If `last_heartbeat` is older than 15s (5s × 3 retries), it marks the plugin as `needs_restart`, kills the child, and re-runs `spawn()`. After 3 restarts in 60s, it logs an error and stops trying (the plugin row is left in `last_error` state; the user can fix it via `agentd plugin start`).

`plugin.heartbeat` from the SDK becomes a no-op for the counter wire (Plan 2 already returned stub counts); the real counter is wired in the supervisor, not in the protocol.

### 6. Daemon restart re-confirm (§9 restart behavior)

On boot, for every session with `status != finished`:

1. Look up `metadata["plugin"]` → owning plugin name
2. Ensure the plugin is connected (or spawn it via the supervisor)
3. Send a synthetic `session.list_pending` RPC (or a `session.discover` for each pane) to ask the plugin to re-confirm each session
4. Plugin responds with `alive + matches` (re-emit `session.started`), `alive + mismatch` (mark `finished`), or no response in 5s (mark `finished`)

The synthetic RPC is a control message on the plugin UDS — defined in the protocol, not in Plan 3 scope to add new RPCs unless needed. If too much scope creep, defer re-confirm to a follow-up and only do step 1–2 in Plan 3 (just respawn plugins, mark nothing).

**Decision:** for Plan 3, do steps 1–2 (respawn plugins, no synthetic RPC). The plugin's own scan loop (every 5s) will pick up sessions naturally and emit `session.started` for matches. The "mismatch → mark finished" path is a follow-up.

### 7. Write-authority enforcement (-32004)

In `crates/agentd/src/handlers/plugin_handlers.rs::session_report_event`, before any side effect:

```rust
if let Some(owner) = session.metadata.get("plugin").and_then(Value::as_str) {
    if owner != plugin_name {
        return Err(ProtocolError::PluginNotAuthoritative);
    }
}
```

The check covers the mutating event kinds: `session.status_changed`, `session.task_changed`, `session.usage_updated`, `session.finished`. Non-mutating events (`session.message`) skip the check. The session row is loaded once at the top of the handler, so this is a single DB read per event.

### 8. `session.jump` + `session.kill` handlers

The current dispatcher signature is `dispatch(method, params, db) -> MutateResult`. Plan 3 adds a `&dyn Tmux` parameter so jump/kill can act:

```rust
pub fn dispatch(method: &str, params: Value, db: &Db, tmux: &dyn Tmux) -> MutateResult
```

`session.jump` resolves the session's `tmux_session` (already in the row), validates it, then calls `tmux.switch_client(name)`. If the user is not inside a tmux client, `switch-client` returns `NotFound`; the handler returns `InvalidParams` with a hint to attach manually.

`session.kill` resolves the session, calls `tmux.kill_session(name)`, then `mark_finished(id, ts)`. Idempotent: a second call on a finished session returns `SessionNotFound`.

All 9 existing call sites in `tests/handlers_mutate.rs` are updated for the new signature.

### 9. RealTmux

`crates/agentd/src/tmux.rs` gets a `RealTmux` impl. It uses `tokio::process::Command` to shell out to `tmux <subcommand> <args>`. Each method maps to a tmux invocation:

| Method | tmux invocation |
|---|---|
| `new_session(name, cwd)` | `tmux new-session -d -s <name> -c <cwd>` → parse `%N` from stdout |
| `has_session(name)` | `tmux has-session -t <name>` (exit 0 = true) |
| `switch_client(target)` | `tmux switch-client -t <target>` |
| `list_panes()` | `tmux list-panes -a -F '#{session_name} #{pane_id} #{pane_current_path}'` |
| `kill_session(name)` | `tmux kill-session -t <name>` |
| `capture_pane(pane, lines)` | `tmux capture-pane -p -t <pane> -S -<lines>` |

All output is parsed with split-whitespace for known-shape output. `RealTmux::new()` validates `tmux` is in PATH and >= 2.6 (the version that introduced `status-interval 1` reliably).

### 10. CLI: `agentd new <path>`, `list`, `jump`, `kill`, `rename`

A shared `ensure_daemon_running(paths: &Paths)` helper, called at the top of each non-`init`/`daemon`/`uninstall` subcommand:

1. Try `ControlClient::connect(&paths.control_socket_path)` with a 100ms timeout
2. If the connect fails, fork `agentd daemon start --detach` via `Command::new(std::env::current_exe()?)`
3. Poll the socket every 50ms for up to 2s
4. Return `ControlClient` ready to call

`agentd new <path>`:
1. Call `ensure_daemon_running`
2. Send `session.create` RPC with `{cwd, agent_type}` (resolved from `config.toml` `default_agent` or `--agent` flag)
3. Print the new session's id + tmux window name

`agentd list`:
1. Call `ensure_daemon_running`
2. Send `session.list_active` RPC, print a table

`agentd jump <id>`:
1. Call `ensure_daemon_running`
2. Send `session.jump` RPC
3. Print OK (the switch happens in the user's terminal)

`agentd rename <id> <name>`:
1. Call `ensure_daemon_running`
2. Send `session.rename` RPC
3. Print OK

`agentd kill <id>`:
1. Call `ensure_daemon_running`
2. Send `session.kill` RPC
3. Print OK

### 11. `agentd daemon start|stop|restart|status`

- `daemon start [--detach|--foreground]` — runs the boot sequence; `--detach` double-forks and writes the PID file, `--foreground` stays in the calling process
- `daemon stop` — sends `daemon.shutdown` RPC, polls until the control UDS is gone (up to 5s)
- `daemon restart` — stop + start
- `daemon status` — sends `state.snapshot`, prints a one-line summary

`daemon start --detach`:
1. `fork()` → child
2. In child: `setsid()` (new session, detach from controlling tty)
3. In child: `fork()` again → grandchild
4. In grandchild: redirect stdio to `/dev/null`, write PID to `daemon.pid`, exec `agentd daemon start --foreground`
5. In child: exit 0
6. In parent: poll the control UDS up to 2s, print "started, pid N"

macOS-specific: `posix_spawn` would be safer than `fork` for library-loaded processes, but agentd is a small binary with no heavy statics, so `fork` is acceptable.

### 12. Tombstone GC

On boot, after step 3 (migrations), before step 6 (control UDS bind):

```sql
DELETE FROM sessions
WHERE status = 'finished'
  AND finished_at < datetime('now', '-30 days');
```

`DELETE FROM events WHERE session_id NOT IN (SELECT id FROM sessions);` (cascade orphans).

The 30-day window is a constant in `daemon.rs`; the spec mentions a `settings.toml` knob post-v1, not in Plan 3.

## Task list (17 tasks, one commit each)

1. **feat(daemon): `Daemon` struct + boot sequence skeleton (steps 1-3)**
   `crates/agentd/src/daemon.rs` with the struct, `run()`, flock + mkdir + open. Tests for flock collision (second daemon fails to start).

2. **feat(daemon): tombstone GC on boot (step 4)**
   Migration-aware DELETE query; test inserts old + new finished sessions, runs GC, asserts only the new one remains.

3. **feat(daemon): `RealTmux` impl using `tokio::process::Command`**
   All 6 methods; tests use a recorded `tmux` binary (or skip if `tmux` not in PATH, gated behind `#[ignore = "needs real tmux"]`).

4. **feat(daemon): restart re-spawn (step 5, respawn plugins only)**
   On boot, for every non-finished session, ensure its owning plugin is connected. Test inserts a session, runs restart, asserts plugin spawn was called.

5. **feat(daemon): `ControlServer` bound in boot (step 6)**
   `Daemon::run` now binds the control UDS. Test binds, sends a `state.snapshot` RPC, gets back the cache.

6. **feat(daemon): `PluginSpawner` trait + `MockPluginSpawner` (skeleton)**
   Trait + mock impl. No real impl yet.

7. **feat(daemon): `RealPluginSpawner` impl**
   `tokio::process::Command` with `kill_on_drop(true)`. Test verifies a recorded binary gets the right args.

8. **feat(daemon): supervisor `autostart` actually spawns (step 7)**
   Replace the stub. Uses `PluginSpawner`. Test: 2 plugins in manifest, both spawned.

9. **feat(daemon): per-plugin UDS bind + handshake**
   Bind `$XDG_RUNTIME_DIR/agentd/plugin-<name>.sock` before spawn. After spawn, accept the plugin's `plugin.hello` and record `last_connected_at`.

10. **feat(daemon): plugin heartbeat counter + 15s timeout**
    `PluginHeartbeat` struct. Background task wakes every 5s, restarts plugins with stale `last_heartbeat`. Test fast-forwards 16s, asserts restart.

11. **feat(daemon): `-32004` write-authority enforcement**
    In `session_report_event`, load session once, check `metadata["plugin"] == plugin_name`. Test: plugin A sends `status_changed` for plugin B's session, gets `-32004`.

12. **feat(daemon): thread `&dyn Tmux` through mutate dispatcher**
    Add `tmux: &dyn Tmux` parameter to `dispatch()`. Update 9 call sites. Test for jump and kill that pass.

13. **feat(daemon): `session.jump` + `session.kill` handlers**
    Real impls. Test: jump on missing session returns `SessionNotFound`; kill on a real (MockTmux) session removes the tmux session + marks finished.

14. **feat(daemon): `ensure_daemon_running` helper**
    Polls control UDS, forks if absent, polls 2s. Used by every non-`init`/`daemon`/`uninstall` subcommand.

15. **feat(daemon): `agentd daemon start --foreground`**
    Runs boot sequence in the calling process. Test: starts, accepts a state.snapshot RPC, shuts down on signal.

16. **feat(daemon): `agentd daemon start --detach` (double-fork)**
    Detached mode. Test: parent forks, child exits 0, grandchild runs boot, PID file written.

17. **feat(daemon): CLI wiring for `new`, `list`, `jump`, `kill`, `rename` + `daemon stop|restart|status`**
    Replace 5 `println!("not yet implemented")` lines. Each calls `ensure_daemon_running` + the matching RPC. Tests cover the happy path with a `MockControlClient` injected.

## Dependencies (added to `crates/agentd/Cargo.toml`)

- `nix` (already in) — for `flock`, `setsid`, `fork` (raw `libc::fork` since `nix` doesn't expose `fork` cleanly)
- `libc` (already in) — for the raw syscalls
- `tokio` with `process` (already in)

No new external crates.

## What NOT to do

- Don't add TUI / ratatui in this plan
- Don't add the `agentd new --pick` fuzzy picker
- Don't implement per-agent plugins beyond `opencode`
- Don't add `systemd` / `launchd` integration
- Don't add a config hot-reload
- Don't `unwrap()` in non-test code (AGENTS.md rule)
- Don't break `PROTOCOL_VERSION` compat without bumping it (AGENTS.md rule)

## Risks

- **fork + Tokio runtime**: forking a process that holds a Tokio runtime is unsafe (other threads are mid-syscall). The double-fork in `daemon start --detach` must `fork` BEFORE entering `Daemon::run`. The detached child re-execs the binary, so the runtime starts fresh.
- **flock + lockd**: `flock` on a closed file descriptor across forks is safe on Linux, behaves differently on macOS. Use `nix::fcntl::flock` with `LockFlags::LOCK_EX | LOCK_NB` and fall back to "is another daemon running?" detection via the control UDS being already bound.
- **MockPluginSpawner testing the restart loop**: the mock has to actually kill the fake child, which means the supervisor's restart code needs to handle a "child already exited" case. Test the supervisor's logic by feeding it a `MockChild` that returns `Ok(())` immediately, then asserting the spawn was retried.
- **Per-plugin UDS ownership race**: the plugin child connects to the UDS, but the daemon binds first. If the plugin's UDS path already exists from a prior crash, bind fails. The supervisor must `unlink` the socket path before bind, and tolerate the unlink failing with `NotFound`.

## Open questions (defer to spec review)

- Should `daemon stop` use a graceful shutdown signal (SIGTERM + grace period) or just `daemon.shutdown` RPC? The RPC is cleaner; the signal is needed if the daemon is wedged. RPC only for v1.
- Should `agentd list` go through the daemon (RPC) or read the DB directly (no daemon required)? The spec says RPC, but a read-only DB path is faster. RPC only for v1; the read-only fallback in `status` already shows the pattern.
- Should tombstone GC be opt-in via a flag? Spec says always-on at boot. Always-on for v1.
