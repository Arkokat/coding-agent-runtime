# agentd — design

**Date:** 2026-06-18
**Status:** Design (post-brainstorm, pre-plan)
**Author:** brainstorm session with user

## Goal

A persistent Rust daemon that orchestrates multiple coding-agent sessions (opencode, Claude Code, Codex, Aider) inside tmux. Exposes a tmux-aware status line, a live ratatui dashboard, and a CLI. Single user, single machine, v1.

## Decisions (locked from brainstorm)

| # | Decision | Rationale |
|---|---|---|
| 1 | v1 target: generic framework, no real-agent plugin yet | Plugin SDK + daemon + dashboard shipped; real plugins built after API stabilizes |
| 2 | Dashboard: TUI in tmux popup | tmux-native, zero port, hotkey-driven |
| 3 | Plugins: out-of-process binaries | Crash isolation, language-agnostic long-term |
| 4 | Session creation: hotkey + auto-detect | `prefix+m` for explicit; plugin scans for orphaned agent runs every 5s |
| 5 | State: SQLite for everything | Survives restart, transactional, indexed, single binary |
| 6 | CLI surface: full + TUI | `agentd list/new/jump/rename/status/kill/doctor/plugin` in any shell |
| 7 | Distribution: cargo + brew + curl, all channels | User picked all three |
| 8 | Plugins: managed by `agentd plugin install` | Single binary for end users; plugins downloaded by tool, not user |
| 9 | Wire protocol: JSON-RPC 2.0 over UDS, shared `agentd-protocol` crate | Types drift impossible, debuggable with `socat` |
| 10 | Tmux control: `tmux_interface` crate v0.4 (19K dl) | Replaces hand-rolled wrapper, typed builders |
| 11 | Daemon startup: lazy via CLI + explicit `agentd daemon` | No systemd/launchd v1 |
| 12 | Status line: shell-call via `status-interval 1` | 1s refresh, in-memory cache, <50ms p99, <1s hard budget |
| 13 | Dashboard layout: header + list + detail + footer | Standard, resizable detail |
| 14 | Color palette: 4 semantic + 2 neutral, ANSI 256 | Minimal, color-blind safe via symbols, `--no-color` escape |
| 15 | Live tests: release-gate (3-layer plan) | Test-agent fixture + recorded-replay + Ollama; real-API for Claude Code |
| 16 | Metrics: in-memory + `agentd metrics` + `agentd debug bundle` | Prometheus text + OTLP/JSON formats shipped; no endpoint v1, strict PII defaults |

## 1. Architecture

```
                    tmux session(s)
                         |
                    prefix+m hotkey
                         |
                         v
                +-------------------+
                |   agentd CLI      |
                +-------------------+
                         | UDS (control)
                         v
+----------+   spawn    +-------------------+   UDS (JSON-RPC)   +-------------------+
| plugins  | <--------- |      agentd       | <----------------> | agent-plugin-X    |
| .toml    |            |     (daemon)      |                     +-------------------+
+----------+            |                   |                              |
                        |  - SQLite         |                              v
                        |  - tmux control   |                     opencode / Claude / ...
                        |  - event bus      |                     (via tmux_interface)
                        +-------------------+
                              ^
                              | UDS (events)
                              |
                        +-------------------+
                        |   agentd-tui      |  <-- ratatui, opened via tmux popup
                        +-------------------+
```

Crate layout (workspace):

- `agentd-protocol` — JSON-RPC types, method constants, error codes. No deps beyond `serde`.
- `agentd` — daemon binary. Subcommands: `daemon`, `tui`, `list`, `new`, `jump`, `rename`, `status`, `kill`, `doctor`, `plugin`, `init`, `metrics`, `debug`. Deps: tokio, rusqlite, tmux-interface, ratatui, clap, nucleo, dialoguer, parking_lot.
- `agentd-tui` — binary (currently `agentd tui` subcommand of `agentd`). Could split post-v1.
- `agent-plugin-sdk` — plugin helper crate (UDS client, mock backend, parsing helpers). Optional.
- `agent-plugin-opencode`, `agent-plugin-claude-code`, `agent-plugin-codex`, `agent-plugin-aider` — per-agent plugins. None shipped v1; SDK + first reference plugin only.
- `agentd-testing` — test harness: `TestHarness`, scripted test-agent fixture, HTTP recording/replay.

## 2. Plugin adapter model

### Generalize (lives in daemon, normalized across all agents)

**Session fields:**
- `id` (UUID v7, daemon-assigned)
- `agent_type` (string, e.g. `opencode`)
- `working_dir` (path)
- `tmux_session`, `tmux_pane_id` (optional)
- `display_name` (renamable, defaults to `basename(working_dir)`)
- `status` (enum: `starting | idle | working | waiting_for_user | errored | finished`)
- `current_task` (string, plugin-supplied)
- `model` (string)
- `context_used_tokens`, `context_total_tokens` (optional)
- `cost_usd` (running total)
- `source` (`cli | discovered | resumed`)
- `created_at`, `last_event_at`, `finished_at` (ISO 8601 UTC)
- `metadata` (JSON blob for plugin extras: git_branch, agent_version, etc.)

**Event types plugin emits (only):**
- `session.discovered`
- `session.started`
- `session.status_changed`
- `session.task_changed`
- `session.usage_updated`
- `session.message`
- `session.finished`

### Specialize (stays in plugin, daemon never sees)

- Spawn args for the agent
- Output capture (stdout / log tail / vendor IPC / hooks)
- Event parser (regex / JSON / vendor schema)
- Status inference heuristic
- Vendor config (model, approval mode)

### Plugin SDK

- `Backend` trait with `real()` and `mock()` impls
- `AGENTD_TEST_MODE=1` env var swaps in mock backend
- Mock = scripted event sequence from JSON/YAML
- Real = whatever the agent needs

### Plugin manifest (`~/.config/agentd/plugins.toml`)

```toml
[[plugin]]
name = "opencode"
binary = "agentd-plugin-opencode"   # $PATH lookup, or absolute path
autostart = true
config = { model = "claude-sonnet-4-5" }
```

## 3. E2E testing (3-layer, release-gated)

| Layer | Trigger | Mechanism | Cost |
|---|---|---|---|
| **Unit** | every PR | parser tests with byte fixtures | $0 |
| **Integration** | every PR | `TestHarness` + scripted test-agent | $0 |
| **Live, recorded** | release | VCR HTTP replay for each plugin | $0 after first record |
| **Live, Ollama** | release | opencode/aider/codex pointed at local Ollama | $0 |
| **Live, real API** | release | Claude Code + Haiku, smoke only | cents |

**`agentd-testing` crate provides:**
- `TestHarness::new(config) -> Harness` — temp dir, daemon on UDS, cleanup on drop
- `TestAgent` — fixture binary. Reads script (`{emit: event, after_ms: N}`), exits on EOF.
- `ScriptedSession` builder
- HTTP recording proxy (for VCR layer)

**CI:**
- PR: unit + integration, all 4 plugin SDKs against test-agent. <8 min.
- Nightly: + recorded-replay + Ollama suite. <30 min.
- Release: + real-API smoke + benchmarks. <60 min. **Release fails if any of these fail.**

## 4. Components & data flow

### Components

1. **`agentd` (daemon)** — long-lived. Owns SQLite. Spawns plugins. Bridges to tmux via `tmux_interface` crate. Exposes control UDS at `$XDG_RUNTIME_DIR/agentd/control.sock`.
2. **`agentd-tui`** — ratatui binary (subcommand `agentd tui`). Connects to control UDS as observer. Receives event notifications via JSON-RPC push.
3. **`agentd` CLI** — clap subcommands. Talks control UDS.
4. **`agent-plugin-X`** — per-agent binary. Spawned by daemon. Talks plugin UDS.
5. **tmux** — external, controlled via `tmux_interface` typed builders.
6. **coding agent** — external, owned by plugin (user-spawned path detected by plugin's scan loop).

### Data flows

| # | Trigger | Path |
|---|---|---|
| 1 | Hotkey `prefix+m` | tmux → `agentd new --cwd $PWD` → daemon: create tmux session + spawn plugin → plugin spawns agent → plugin emits `session.started` → daemon persists, broadcasts |
| 2 | User manually starts agent in existing tmux window | plugin scan loop (every 5s) lists panes, matches agent binary → `session.discovered` → daemon registers |
| 3 | Agent outputs event | stdout/log → plugin parser → `session.status_changed` / etc. → daemon writes SQLite, broadcasts |
| 4 | User renames in TUI | TUI → `session.rename` RPC → daemon updates row → broadcast |
| 5 | User jumps | TUI → `session.jump` RPC → daemon runs `tmux switch-client -t <session>` (or `attach-session` outside tmux) |
| 6 | Daemon restart | daemon reads SQLite → re-spawns plugins → plugins scan tmux → confirm or mark `finished` → TUI re-subscribes |

### Trust boundaries

- Control UDS: 0600 perms, owned by user. CLI/TUI authenticate via socket file ownership.
- Plugin UDS: daemon enforces `plugins.toml` allowlist, peer uid matches user.
- tmux CLI: daemon validates all session/pane names before passing to `tmux_interface` (no shell injection).

### Daemon startup

**4 paths:**
- **Lazy** — every `agentd *` pings control UDS first. If absent, forks `agentd daemon --detach`, polls socket up to 2s, then proceeds.
- **Explicit** — `agentd daemon start` (default `--detach`) or `agentd daemon start --foreground` for debug.
- **tmux hotkey** — `prefix+m` → `run-shell "agentd new --cwd $PWD"` → triggers lazy path.
- **Systemd/launchd** — post-v1.

**Boot sequence:**
1. `flock(LOCK_EX)` on `$XDG_RUNTIME_DIR/agentd/daemon.lock` (no double-daemon)
2. `mkdir 0700` on `$XDG_RUNTIME_DIR/agentd/`
3. Open SQLite at `$XDG_DATA_HOME/agentd/state.db`, run migrations
4. Bind control UDS
5. Read `plugins.toml` → spawn each `autostart=true` plugin
6. For each persisted session with `status != finished`: re-spawn plugin if dead, ask it to re-confirm, mark `finished` if gone
7. Idle loop

## 5. tmux status line

**Two surfaces, one mechanism: shell command via `status-interval 1`.**

**Global `status-right`** (aggregate):
```
set -g status-interval 1
set -g status-right "#(agentd status --global)"
```
`agentd status --global` returns e.g. `5 agents · 2 waiting · 1 working · $0.42`.

**Per-pane border** (per-agent):
```
set -g pane-border-status bottom
set -g pane-border-format "#{?pane_active,#[bold],}#(agentd status --pane '#{pane_id}' 2>/dev/null)"
```
Returns e.g. `claude · editing src/foo.rs`.

**Performance budget (hard):**
- Each call: cold start ~5ms, p99 <50ms. **Hard limit: 1s.** Daemon logs warning above 500ms.
- In-memory `RwLock<HashMap<PaneId, FormattedStatus>>` rebuilt on every event. Status call = read lock + format. **Never touches SQLite. Never makes outbound calls.**
- Unknown pane → empty string in <1ms.
- Refresh interval: 1s (very live, accepted default).
- Daemon down → calls return empty, panes blank (graceful).

## 6. Live dashboard (`agentd tui`)

**ratatui, single UDS connection full-duplex, 30fps debounced render.**

**Connection flow:**
1. Connect to control UDS
2. `state.snapshot` request → initial state
3. `subscribe` notification (`events: ["session.*", "plugin.*"]`)
4. Daemon pushes `event` notifications
5. `tokio::select!` over UDS reader + keyboard

**Event types pushed:** `session.created/renamed/finished/killed/status_changed/task_changed/usage_updated/message`, `plugin.connected/disconnected/error`, `daemon.shutting_down`.

**Render loop:** state updated sync on event; render coalesced at 16ms (30fps); dirty row tracking via `Buffer` diff; status changes flash row for 500ms then fade.

**Layout (3 panes):**
```
+--------------------------------------+
| 5 agents · 2 waiting · 1 working     |  <- header
+--------------------------------------+
| ●  claude-sonnet-4   working  edit.. |  <- session list
| ◌  gpt-4o            idle            |
| ⚠  claude-sonnet-4   waiting  need.. |
| ✕  ollama            errored  ...    |
+--------------------------------------+
| claude-sonnet-4 in ~/proj/foo        |  <- detail
|   task: editing src/main.rs          |
|   tokens: 42k/200k (21%)             |
|   cost: $0.12                        |
|   last event: tool_call (Edit)       |
+--------------------------------------+
|  c=create  r=rename  j=jump  q=quit  |  <- footer
+--------------------------------------+
```

**Keys:** `j/k` move, `g/G` top/bottom, `Enter` jump, `r` rename, `c` new, `x` kill, `?` help, `q` quit.

**Live signals:** row invert on status change, `⚠` pulse on waiting, no spinners/bars.

## 7. Color palette (minimal)

**4 semantic + 2 neutral. ANSI 256. No truecolor, no themes, no backgrounds.**

```
working    green    AnsiValue 71
waiting    yellow   AnsiValue 178
errored    red      AnsiValue 167
idle       gray     AnsiValue 244
text       default
muted      default dim
```

**Symbols (always present, color-independent):**
- `●` working
- `◌` idle
- `⚠` waiting
- `✕` errored

**Selection:** reverse video, terminal-native.
**Escape:** `--no-color` falls back to symbols only.
**Auto-detect:** `--color=auto` checks `isatty(STDOUT)`.

**Where:** TUI via ratatui + crossterm; status line via hand-rolled ANSI (no `colored` crate).

## 8. Data model (SQLite)

**Path:** `$XDG_DATA_HOME/agentd/state.db`. WAL mode. `rusqlite` + `spawn_blocking`. Single writer (daemon), many readers (CLI).

```sql
CREATE TABLE schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE sessions (
  id                   TEXT PRIMARY KEY,          -- UUID v7
  agent_type           TEXT NOT NULL,
  working_dir          TEXT NOT NULL,
  tmux_session         TEXT,
  tmux_pane_id         TEXT,
  display_name         TEXT NOT NULL,
  status               TEXT NOT NULL,             -- starting|idle|working|waiting_for_user|errored|finished
  current_task         TEXT,
  model                TEXT,
  context_used_tokens  INTEGER,
  context_total_tokens INTEGER,
  cost_usd             REAL,
  source               TEXT NOT NULL,             -- cli|discovered|resumed
  created_at           TEXT NOT NULL,
  last_event_at        TEXT,
  finished_at          TEXT,
  metadata             TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_sessions_status_event ON sessions(status, last_event_at DESC);
CREATE UNIQUE INDEX idx_sessions_tmux
  ON sessions(tmux_session, tmux_pane_id)
  WHERE tmux_session IS NOT NULL AND tmux_pane_id IS NOT NULL;
CREATE INDEX idx_sessions_agent ON sessions(agent_type);

CREATE TABLE session_events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  type       TEXT NOT NULL,
  payload    TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_events_session_time ON session_events(session_id, created_at DESC);

CREATE TABLE plugins (
  name              TEXT PRIMARY KEY,
  binary            TEXT NOT NULL,
  socket_name       TEXT NOT NULL,
  autostart         INTEGER NOT NULL DEFAULT 1,
  last_connected_at TEXT,
  last_error        TEXT
);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
-- default rows:
--   scan_interval_secs=5, status_interval_secs=1, default_agent='opencode',
--   auto_detect=1, last_agent='opencode', slow_status_warn_ms=500
```

**Migrations:** `migrations/0001_init.sql`, `0002_*.sql`, ... applied in order, each in a transaction.

**Why UUID v7:** time-ordered, no central service.
**Why `metadata` JSON:** keeps plugin-specific extras out of schema; indexed later if needed.
**Why append-only events:** replay, postmortem, "what did this session do" history.

## 9. Session lifecycle

```
                 +-----------+
                 | starting  |
                 +-----+-----+
                       |
                       v
       +-------+   +---------+   +------------------+
       | idle  |<--+ working +-->| waiting_for_user |
       +---+---+   +----+----+   +---------+--------+
           ^            |                  |
           | (5m idle)  |                  | (user responds)
           +------------+                  |
                                            v
                                        working
       any --[errored event]--> errored   (sticky)
       any --[agent exit]-----> finished  (terminal, immutable)
```

**Write authority (hard rule):** only the owning plugin emits status changes. Daemon rejects non-owning plugin's `status_changed` (-32004).

**Lifecycle events:**

| Event | Source | Side effects |
|---|---|---|
| `session.create` | CLI or plugin `discover` | INSERT row, `status=starting`, generate UUID v7, bind plugin |
| `session.started` | plugin | `starting → working` or `→ errored` |
| `session.status_changed` | plugin | UPDATE status + last_event_at |
| `session.task_changed` | plugin | UPDATE current_task |
| `session.usage_updated` | plugin | UPDATE context_* + cost_usd |
| `session.message` | plugin | INSERT into session_events (no row update) |
| `session.finished` | plugin | UPDATE `status=finished`, `finished_at=now` |
| `session.dismiss_error` | CLI/TUI | UPDATE `errored → idle` (only if plugin still alive) |

**Daemon restart behavior:**
1. Read all sessions with `status != finished`
2. Ensure owning plugin connected; spawn if not
3. Plugin scans tmux + agent process:
   - Alive, matches → `session.started` to confirm, daemon re-emits current status
   - Alive, doesn't match → mark `finished`
   - Gone → mark `finished`
4. `errored` stays sticky until user dismisses

**Tombstones:** finished sessions kept 30 days, GC'd on daemon boot. TUI has `--all` to show.

## 10. IPC protocol (JSON-RPC 2.0)

**Framing:** NDJSON (newline-delimited JSON). `serde_json::to_string` per message. NDJSON over length-prefixed: payload <8KB, no embedded newlines, simpler.

**Two UDS:**

| UDS | Path | Speakers | Direction |
|---|---|---|---|
| Control | `$XDG_RUNTIME_DIR/agentd/control.sock` | CLI, TUI → daemon | request/response + server-pushed events |
| Plugin | `$XDG_RUNTIME_DIR/agentd/plugin-<id>.sock` | plugin → daemon | request/response only |

### Control UDS methods (client → daemon)

```rust
// read-only
state.snapshot() -> { sessions: [...], plugins: [...] }
session.get(id) -> Session
session.events(id, since?: ISO8601) -> [Event]
daemon.status() -> { uptime, version, session_count, plugin_count }
plugin.list() -> [Plugin]
metrics(section?: String) -> Metrics
metrics.export({ format: "prometheus"|"otlp"|"json"|"text" }) -> String  # streamed to caller

// mutations
session.create({ agent_type, working_dir, name? }) -> Session
session.rename(id, display_name) -> Session
session.jump(id) -> { ok, switched: bool }
session.kill(id) -> { ok }
session.dismiss_error(id) -> Session
plugin.install(name) -> { ok, version }
plugin.update(name?) -> { ok, updated: [name] }
plugin.remove(name) -> { ok }
plugin.start(name) -> { ok }
plugin.stop(name) -> { ok }
daemon.shutdown() -> { ok }

// pub/sub
subscribe({ events: ["session.*", "plugin.*"] }) -> { subscription_id }   // notification
unsubscribe({ subscription_id }) -> { ok }
```

### Plugin UDS methods (plugin → daemon)

```rust
plugin.hello({ name, version, pid, binary_path })
  -> { ok, plugin_id, heartbeat_interval_secs: 5 }
session.report_event({ session_id, type, payload, ts })
  -> { accepted: bool, event_id }
session.discover({ tmux_session, tmux_pane_id, working_dir, initial })
  -> { session_id, ok }
plugin.heartbeat() -> { ok, events_total, invalid_total, restart_required: bool }
plugin.bye() -> { ok }
```

### Server-pushed events (daemon → subscribers)

JSON-RPC notifications, no `id`:
```json
{"jsonrpc":"2.0","method":"event","params":{"type":"session.status_changed","session":{...},"ts":"..."}}
```

### Error codes

| Code | Meaning |
|---|---|
| -32700 | Parse error |
| -32600 | Invalid request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32603 | Internal error |
| -32001 | Session not found |
| -32002 | Plugin not allowed |
| -32003 | Permission denied (uid mismatch) |
| -32004 | Plugin not authoritative |
| -32005 | Daemon shutting down |

### Auth

- Socket files 0600, owner = daemon's uid
- `getpeereid()` / `SO_PEERCRED` / `LOCAL_PEERPID` per accept
- `plugin.hello` must present name in `plugins.toml` AND binary path matches what daemon spawned

### Timeouts

- daemon → plugin: 5s normal, 30s for `discover`
- plugin → daemon: 2s for `report_event`
- daemon → CLI: 5s default
- All configurable via env

## 11. Distribution & config

### Filesystem layout (XDG)

```
~/.config/agentd/
  config.toml
  plugins.toml
  tmux.conf.fragment

~/.local/share/agentd/
  state.db
  state.db-wal
  state.db-shm
  logs/{daemon,plugin-<name>}.log
  plugins/agentd-plugin-<name>     # downloaded by `agentd plugin install`
  debug-bundle-<host>-<ts>.tar.gz

~/.cache/agentd/
  status-global.txt                # optional fast-path

$XDG_RUNTIME_DIR/agentd/
  daemon.lock
  control.sock
  plugin-<name>.sock
  plugin-<name>.pid
```

### `agentd init` (first-run)

1. Check tmux on PATH, version ≥ 2.6
2. Create `~/.config/agentd/` + default `config.toml` + empty `plugins.toml`
3. Print tmux.conf fragment
4. Offer to append to `~/.tmux.conf` (with backup)
5. Print next-steps: `agentd plugin install <name>`

### `config.toml`

```toml
[daemon]
scan_interval_secs = 5
status_cache_path = true
log_level = "info"

[ui]
default_agent = "opencode"
color = "auto"
status_interval_secs = 1

[experimental]
e2e_live_tests = false
```

### `plugins.toml`

```toml
[[plugin]]
name = "opencode"
binary = "agentd-plugin-opencode"
autostart = true
config = { model = "claude-sonnet-4-5" }
```

### Distribution (single-binary for end users)

| Channel | Steps |
|---|---|
| **curl** | `curl -fsSL https://agentd.dev/install.sh \| sh` then `agentd plugin install opencode` |
| **brew** | `brew install agentd` then `agentd plugin install opencode` |
| **cargo** | `cargo install agentd` then `agentd plugin install opencode` |

**Plugin model:**
- `agentd plugin install <name>` downloads from `agentd-plugins/<name>` GitHub releases, verifies SHA, stores in `~/.local/share/agentd/plugins/`, registers in `plugins.toml`
- `agentd plugin update` updates all
- `agentd plugin list` shows installed + available
- Plugin authors: `cargo install --path crates/agent-plugin-X` for dev; end users don't touch cargo for plugins

### Versioning

- Workspace version: `0.1.0` v1
- `agentd-protocol::PROTOCOL_VERSION` constant
- Daemon refuses plugin with `PROTOCOL_VERSION > daemon's` (forward compat)
- Daemon accepts plugin with `PROTOCOL_VERSION < daemon's` + warn (backward compat)
- Semver: protocol minor = additive, major = breaking

### Upgrade

1. Update binary (`cargo install --force` / `brew upgrade` / re-run curl)
2. `agentd daemon restart` (or restart on mtime change, post-v1)
3. Migrations run, plugins respawn

## 12. New session flow

**Single-screen fuzzy picker. Type-to-filter recents OR type-to-custom. No stepped wizard.**

**Trigger:** `c` in TUI, or `prefix+m` (which calls `agentd new --pick`).

**Modal:**
```
┌─ New session ──────────────────────────┐
│  █                                      │  <- search input
│                                         │
│  Recent:                                │
│  > ~/projects/agentd                    │  <- highlighted
│    ~/work/api-server                    │
│    ~/personal/blog                      │
│    ~/sandbox/test                       │
│                                         │
│  Agent: opencode  ▾                     │
│                                         │
│  ⏎ create   ⎋ cancel                    │
└─────────────────────────────────────────┘
```

**Behavior:**
- Open → empty input, recents unfiltered, top selected
- Type → fuzzy filter recents (`nucleo`, matches basename + full path)
- ↑/↓ → move selection
- Enter → selection OR typed path
- Agent dropdown → `default_agent` pre-selected, ↓ cycles plugins
- Esc → cancel

**Recents source (live query):**
```sql
SELECT working_dir, MAX(last_event_at) AS last_used
FROM sessions
WHERE status != 'finished' OR finished_at > datetime('now', '-30 days')
GROUP BY working_dir
ORDER BY last_used DESC
LIMIT 10;
```

**Path normalization:** `realpath()`, trailing slash stripped, non-existent allowed.

**CLI:**
- `agentd new <path>` — direct, fastest
- `agentd new` — interactive picker in TTY (`dialoguer` + `nucleo`)
- `agentd new --pick` — always interactive
- `agentd new --recent` — list recents, pick by number

**Persistence:** `settings.last_agent` updated on each create, used as default next time.

**Speed budget (from keypress to visible new pane):**
- Modal open: <50ms
- Fuzzy filter: <5ms
- Path resolve: <10ms
- RPC: <20ms
- tmux new-session: <50ms
- Plugin spawn: <100ms
- **Total: <300ms**

**No path validation at pick time** — if path is read-only / non-existent, tmux errors, daemon reports, no row created.

## 13. Error handling

**Principle: every failure detected, categorized, reported, self-heals where possible. Never silent. Never panicking.**

### Categories

| Category | Examples | Behavior |
|---|---|---|
| Startup | tmux not found, SQLite unwritable, bad config | Exit with diagnostic + fix hint |
| Runtime (recoverable) | plugin disconnect, status slow, scan timeout | Self-heal / retry / warn |
| Runtime (terminal) | SQLite corruption, disk full, OOM | Degrade to read-only, big banner |
| User input | bad RPC params, invalid rename | JSON-RPC error, no state change |
| Security | peer uid mismatch, plugin not allowlisted | Reject, log with peer info |

### Per-failure handling

| # | Failure | Detection | Response |
|---|---|---|---|
| 1 | Daemon not running | CLI/TUI: connect fails | Lazy start; if fails, `agentd daemon start` hint |
| 2 | Daemon crashes | UDS peers see disconnect | CLI/TUI: "restart?"; restart re-validates |
| 3 | Plugin crashes | UDS disconnect + heartbeat timeout (5s × 3) | Restart with backoff (1s, 5s, 30s). After 3 fails, mark `failed` |
| 4 | tmux not running | `tmux has-session` non-zero | Fail fast: `tmux not running. Start: tmux new-session -d` |
| 5 | tmux server dies | `has-session` on known session | Plugin's next scan marks `finished` |
| 6 | SQLite disk full | `SQLITE_FULL` | Read-only mode, no new sessions, banner |
| 7 | SQLite corruption | `PRAGMA integrity_check` fails | Refuse to start. `agentd doctor` offers copy+init fresh |
| 8 | Plugin UDS auth fail | `getpeereid()` mismatch | Reject, log `WARN peer uid=X expected=Y` |
| 9 | Status call slow | p99 over 500ms | Log warning, set `slow_status=true` |
| 10 | Status call timeout | exceeds 1s | Return "stale" marker, doesn't block next interval |
| 11 | Two daemons | `flock` fails | Second exits with `agentd already running (pid=Y)` |
| 12 | Stale lock | PID in lock file dead | Take over, log `reclaimed stale lock from pid X` |
| 13 | Config parse fail | `toml::from_str` error | Exit with line/column, suggest `agentd doctor` |
| 14 | Invalid plugin event | schema validation | Reject `-32602`, plugin logs + retries. 10 invalid in 60s → disconnect |
| 15 | Agent binary missing | plugin `exec` fails | Plugin exits non-zero, sessions `errored`, TUI: "opencode not on PATH" |
| 16 | User kills agent | `child.wait()` returns | Plugin emits `session.finished`, daemon cleans up |
| 17 | Clock skew | NTP drift | All timestamps UTC ISO 8601; no cross-machine compares (v1 single-machine) |

### Logging

- `~/.local/share/agentd/logs/daemon.log` — JSON lines, rotated 10MB × 3
- `~/.local/share/agentd/logs/plugin-<name>.log` — same
- Levels: `trace | debug | info | warn | error`. Default `info`.
- Override: `AGENTD_LOG=debug` or `config.toml`

### Error type

```rust
struct AgentdError {
    code: ErrorCode,         // enum: PluginNotFound, TmuxVersionTooOld, ...
    message: String,
    cause: Option<Box<dyn Error>>,
    recoverable: bool,
}
```

### `agentd doctor` checks

- tmux installed + version ≥ 2.6
- Daemon running (UDS ping)
- All configured plugins reachable on UDS
- SQLite readable + writable + integrity OK
- Config files parse
- Socket dir writable + 0600
- Disk space > 100MB free
- Each plugin's agent binary on PATH
- Returns 0 green, 1 any red, prints fix hints

### Degradation order

1. **Healthy** — full functionality
2. **Some plugins down** — others unaffected, errored sessions visible
3. **Daemon readonly** — no new sessions, status line works from memory
4. **TUI can reach daemon, no tmux** — cached state, no jump
5. **All gone** — TUI error screen, "agentd daemon start"

## 14. Testing per component

**3-layer strategy (Section 3) applied per component. Test pyramid: unit → integration → live (release-gate).**

### Coverage targets

| Component | Target | Tooling |
|---|---|---|
| `agentd-protocol` | 95%+ | cargo test |
| `agentd` daemon core | 85%+ | cargo test + proptest |
| Tmux layer | 100% integration | real tmux in temp env |
| Status generator | p99 < 500ms | criterion |
| TUI | 75% + snapshots | ratatui test backend + insta |
| CLI | 80% | assert_cmd + predicates |
| Plugin SDK | 90% | unit + harness |
| Plugins | 80% parser | unit + harness + live |
| Test harness | 100% integration | full lifecycle |
| Installer | 100% init flow | tmpdir + docker |

### Per-component specs

1. **`agentd-protocol`**: serde roundtrip, parse/invalid-request errors, `PROTOCOL_VERSION` mismatch.
2. **`agentd` daemon**: migrations forward+down, state machine (`proptest` random sequences), flock, event broadcast, subscribe/unsubscribe, full lifecycle via harness.
3. **Tmux layer**: command builders (no real tmux), integration against real tmux in temp dir for every method.
4. **Status generator**: format per status, empty pane, criterion perf bench.
5. **`agentd-tui`**: `insta` snapshots at 80×24 and 200×50 for: empty, each status, 10 mixed, errored overlay, help modal, new modal (empty/filtering/selected), rename modal. Keyboard integration.
6. **CLI**: `assert_cmd` per subcommand. Snapshot output (`--color=never`).
7. **Plugin SDK**: `MockBackend` roundtrip, `RealBackend` trait compile-test, end-to-end in test mode.
8. **Plugins**: parser fixtures in `tests/fixtures/agent-output/<agent>/*.jsonl`, status inference, cost calc, harness integration with mock, VCR replay (release), Ollama (release), real API smoke (release).
9. **Test harness**: 100 concurrent sessions, property test random events, cleanup on drop.
10. **Installer**: `agentd init` smoke in tmpdir, curl install in docker, brew formula in CI.

### CI matrix

| Stage | Triggers | Contents | Budget |
|---|---|---|---|
| PR | every push | unit + integration + snapshots, 4 SDKs × mock, linux + macOS | <8 min |
| Nightly | cron | + live recorded-replay + Ollama | <30 min |
| Release | tag | + real-API smoke + benchmarks | <60 min |

### Test execution

- `cargo nextest run` (parallel, fast)
- `cargo nextest run --profile ci` (fail-fast)
- `cargo bench` (nightly)
- `cargo miri run` (nightly, unsafe audit)

### `insta` snapshot workflow

- PRs touching TUI render produce snapshot diffs
- Reviewer runs `cargo insta review`
- Snapshots committed on approval

### Fixtures (in repo)

- `crates/agentd-testing/fixtures/agent-output/{opencode,claude-code,codex,aider}/*.jsonl`
- `crates/agentd-testing/fixtures/http/{...}/*.har` (VCR format)
- `crates/agentd-testing/fixtures/recorded-sessions/*.json`

## 15. Metrics & debug bundle

**v1 metrics: two CLI commands. No external endpoint. PII-safe by default.**

### `agentd metrics`

```
$ agentd metrics
Sessions:    12 total — 2 waiting, 5 working, 4 idle, 1 errored
Plugins:     3/4 connected (claude-code disconnected 2m ago)
IPC:         47 conn, 1.2k rpc/min, p50 2ms p99 18ms
Tmux:        12 sessions, 34 panes, p50 5ms p99 41ms
SQLite:      1.2 MB, WAL 8 KB, queries p50 1ms
Status line: p50 3ms p99 12ms, 0 slow, 0 stale (1h)
Daemon:      up 4h 12m, 28 MB rss, 4 threads
```

Flags:
- `--format text|json|prometheus|otlp` (default `text`)
- `--watch` (1s refresh, until Ctrl-C; works with any format)
- `--section <name>` (one of: sessions, plugins, ipc, tmux, sqlite, status, daemon)

### Standard output formats

| Format | Flag | Compatibility | Use case |
|---|---|---|---|
| **text** | `--format text` | human | default, terminal reading |
| **json** | `--format json` | machine, custom | debugging, scripting |
| **Prometheus text 0.0.4** | `--format prometheus` | Prometheus, VictoriaMetrics, Grafana Agent, Mimir, Datadog Agent, New Relic, Elastic, Splunk | push to any vendor |
| **OTLP/JSON** (OpenTelemetry Protocol) | `--format otlp` | OTel Collector → Grafana Cloud, Honeycomb, Datadog OTel, New Relic OTel, etc. | push through OTel pipeline |

**Both Prometheus and OTLP shipped v1.** Output to stdout. User pipes/redirects.

**Why both:**
- **Prometheus text** = universal, single-file format, every metrics vendor ingests it. Simplest path to "analyze later."
- **OTLP/JSON** = industry-standard, traces+metrics in one pipe, what every big vendor is converging on. Pipe through OTel Collector for vendor-specific shaping.

**Why not embed a network endpoint v1:**
- HTTP server in daemon = more deps (`hyper`), attack surface, lifecycle complexity
- "Agent out, format out" is the standard pattern (`node_exporter` textfile, `cAdvisor`, `pg_exporter`)
- User can run their own scraper (cron + curl, or `prometheus-pushgateway` sidecar, or OTel Collector `prometheus`/`otlpjsonfile` receiver)
- Post-v1: optional `--metrics-port` (Prometheus scrape) and `--otlp-endpoint` (OTLP push)

### Metric naming

Follow Prometheus naming conventions + OpenTelemetry semantic conventions where applicable.

**Counters:**
```
agentd_sessions_total{agent_type,status}
agentd_plugin_events_total{plugin,result}
agentd_rpc_total{method,result,peer}
agentd_rpc_errors_total{method,code}
agentd_tmux_cmd_errors_total{command}
agentd_status_slow_total
agentd_status_stale_total
```

**Gauges:**
```
agentd_daemon_uptime_seconds
agentd_daemon_rss_bytes
agentd_daemon_threads
agentd_sessions_oldest_active_seconds
agentd_tmux_sessions
agentd_tmux_panes
agentd_sqlite_db_bytes
agentd_sqlite_wal_bytes
agentd_ipc_control_connections
agentd_ipc_plugin_connections{plugin}
```

**Histograms** (Prometheus default buckets: 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10):
```
agentd_rpc_duration_seconds{method}
agentd_tmux_cmd_duration_seconds{command}
agentd_status_generation_seconds
agentd_sqlite_query_duration_seconds
```

**Auto-emitted process metrics (OTel semantic conventions):**
```
process_resident_memory_bytes
process_cpu_seconds_total
process_open_fds
process_start_time_seconds
```

### `agentd debug bundle`

- Output: `~/.local/share/agentd/debug-bundle-<host>-<ts>.tar.gz`
- Default cap: 50MB
- Contents: all logs, metrics snapshot (text + json), doctor output, SQLite schema dump (no rows), config files (keys redacted), system info, process info, sanitized session metadata.
- Flags: `--include-session-events`, `--redact-paths`, `--no-redact-config`, `--include-metrics-prometheus` (adds `metrics.prom`), `--include-metrics-otlp` (adds `metrics.otlp`).

### Metrics schema

```rust
struct Metrics {
    daemon:    { uptime_secs, pid, rss_bytes, thread_count },
    sessions:  { total, by_status, by_agent_type, oldest_active_secs },
    plugins:   HashMap<Name, { connected, uptime_secs, events_total,
                               invalid_total, last_event_age_secs,
                               restart_count }>,
    ipc:       { control_conns, plugin_conns, rpc_p50/p95/p99_ms,
                 rpc_errors_total, events_broadcast_total },
    tmux:      { session_count, pane_count, cmd_p50/p99_ms, cmd_errors_total },
    sqlite:    { db_bytes, wal_bytes, query_p50/p99_ms },
    status:    { gen_p50/p99_ms, slow_total, stale_total },
}
```

### Implementation

- `parking_lot::Mutex<Metrics>` for snapshot (single writer)
- Per-RPC timer around every handler
- Sliding window: last 1000 latencies (fixed buckets, HDR-style)
- Reset on daemon start, no persistence

### Plugin metrics

- Reported in `plugin.heartbeat` response (Section 10)
- Daemon merges into its view

### Privacy defaults (strict)

- `metrics` never includes: task text, message content, model name, cost
- `debug bundle` strips all event payloads except type + timestamp
- Config redaction: `(?i)(api[_-]?key|token|secret)\s*=\s*\S+` → `=REDACTED`
- Bundle generation is offline (no network)

### Why no scrape/push endpoint v1

- HTTP server in daemon = more deps, more attack surface, lifecycle complexity
- "Format out, pipe to your own collector" is the standard pattern
- Post-v1: optional `--metrics-port` (Prometheus scrape) and `--otlp-endpoint` (OTLP gRPC push), both off by default

### Testing

- Unit: percentile, counter, redaction regex
- Unit: Prometheus format parses cleanly via `prometheus-parser` crate
- Unit: OTLP/JSON parses cleanly via `opentelemetry-proto`
- Integration: scripted events → expected metrics JSON
- Snapshot: `metrics --json` for known state (insta)
- Snapshot: `metrics --format prometheus` golden file (parsed + re-emitted)
- Roundtrip: `agentd metrics --format prometheus | promtool check metrics` returns OK
- Roundtrip: `agentd metrics --format otlp | otelcol validate` returns OK
- Privacy: `strings bundle.tar.gz | grep -i 'task\|message\|prompt'` returns no PII
- Bundle: size cap enforced, missing files logged not failed

## Open questions / out of scope v1

- Multi-machine sync (Nostr, ssh) — out
- Custom agent definitions (e.g. `agent_type = "my-agent"`) via config — out, SDK is the path
- Pinned paths in recents — out
- TUI themes / palette config — out
- Prometheus scrape endpoint — out (post-v1, `--metrics-port` flag)
- OTLP push endpoint — out (post-v1, `--otlp-endpoint` flag)
- Systemd/launchd user unit — out
- Auto-restart on binary mtime change — out
- Custom plugin registry beyond GitHub releases — out
- `agentd plugin search` (browse registry) — out (use GitHub directly)
- Web UI alternative to TUI — out
- Notifications (terminal bell, desktop notif) — out
- Auto-update of `agentd` itself — out

## Glossary

- **Session** — one tracked coding-agent process, identified by UUID v7, with tmux session/pane binding
- **Plugin** — out-of-process binary that owns one or more agent processes, normalizes events
- **Backend (SDK)** — plugin's `real()` and `mock()` impls of the event source
- **Control UDS** — daemon's socket for CLI/TUI clients
- **Plugin UDS** — daemon's per-plugin socket
- **Recents** — distinct `working_dir` values from past sessions, sorted by last use
- **Tombstone** — `finished` session kept 30 days for history, then GC'd
- **Debug bundle** — offline tarball of logs, metrics, config for bug reports
- **VCR** — HTTP recording/replay for paywall-free live tests
