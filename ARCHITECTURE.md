# Architecture

Quick orientation for engineers. Full design: `docs/superpowers/specs/2026-06-18-agentd-design.md`.

## What agentd is

A persistent Rust daemon that orchestrates multiple coding-agent sessions inside tmux. It:

- Spawns and monitors coding-agent processes (one session per agent run)
- Reports session status to a tmux status line (refreshed every second)
- Provides a ratatui dashboard in a tmux popup
- Exposes a CLI for scripting and automation

## High-level components

```
tmux ──prefix+m──> agentd CLI
                      │
                      │ UDS (control)
                      v
                  agentd daemon ──spawn──> agent-plugin-X
                      │                       │
                      │ SQLite                │ talks to
                      │ tmux_interface        │ opencode / Claude / etc
                      │ event bus
                      │
                      v (events)
                  agentd tui (ratatui)
```

## Crates

- `agentd-protocol` — JSON-RPC 2.0 types. No I/O. Foundation.
- `agentd-testing` — test harness, fixtures, HTTP mock. Foundation.
- `agentd` (planned) — daemon binary. Multi-subcommand CLI + core orchestrator.
- `agentd-tui` (planned) — ratatui dashboard.
- `agent-plugin-sdk` (planned) — plugin helper crate.
- Per-agent plugins (planned, post-v1) — `agent-plugin-opencode`, etc.

## Data flow (simplified)

1. User presses `prefix+m` in tmux
2. tmux runs `agentd new --cwd $PWD`
3. CLI sends `session.create` over control UDS to daemon
4. Daemon creates a tmux session, spawns the appropriate plugin
5. Plugin starts the agent process in the new tmux pane
6. Plugin normalizes agent events and sends them to the daemon
7. Daemon writes to SQLite, broadcasts to all subscribers
8. TUI receives events, updates the dashboard
9. tmux status line calls `agentd status --pane <id>` every second, gets fresh data

## IPC

JSON-RPC 2.0 over Unix domain sockets. NDJSON framing. Two sockets: control (CLI/TUI) and plugin (per-plugin).

See `agentd-protocol` for the type definitions and `docs/superpowers/specs/2026-06-18-agentd-design.md` section 10 for the full method list.

## Persistence

SQLite at `$XDG_DATA_HOME/agentd/state.db` (default `~/.local/share/agentd/state.db`). WAL mode. Schema in spec section 8.

## Distribution

Single binary (`agentd`) with subcommands. Plugins downloaded via `agentd plugin install <name>`. Distribution channels: curl script, Homebrew, crates.io. MIT licensed. 4 build targets.

## Open questions / future work

See spec sections 16, 17, 19 and the "Open questions" section for what's deferred.
