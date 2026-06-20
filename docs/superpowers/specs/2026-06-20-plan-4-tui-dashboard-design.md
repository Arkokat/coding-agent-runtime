# Plan 4 — Live TUI dashboard

## Goal

Build the headline user-facing feature of agentd: a ratatui dashboard (`agentd tui`) that connects to the running daemon's control UDS, shows a live 3-pane view of all sessions + plugins, and supports jump/rename/kill/create via keybindings. After this plan, an agentd user can pop open the dashboard, see all their agent sessions update in real time, and drive them with single-key actions.

## Scope

Everything in spec §6 (Live dashboard) and §7 (Color palette) at "full" depth (3 panes + new modal + rename modal + all keybindings). The TUI is a subcommand of `agentd` (per spec line 69: "binary (currently `agentd tui` subcommand of `agentd`). Could split post-v1"), so it lives in `crates/agentd/src/tui/`.

### In scope

- New `crates/agentd/src/tui/` module (event loop, layout, state, modals)
- New `crates/agentd/src/tui/color.rs` (palette per §7)
- New `crates/agentd/src/tui/state.rs` (in-memory TUI state with dirty-row tracking)
- New `crates/agentd/src/tui/render.rs` (3-pane layout)
- New `crates/agentd/src/tui/input.rs` (keymap)
- New `crates/agentd/src/tui/event_source.rs` (multiplexed ControlClient read)
- New daemon-side: subscribe/unsubscribe/event-push in the control UDS handler
- New `crates/agentd/tests/tui.rs` (insta snapshot tests via ratatui TestBackend)
- 4 daemon-side protocol changes:
  - `subscribe` accepts an `events: [String]` filter
  - `unsubscribe` removes the connection from the subscriber list
  - Server-pushed `event` notifications (JSON-RPC notifications, no `id` field)
  - `daemon.shutting_down` event pushed on graceful shutdown
- CLI subcommand `agentd tui` (replaces the `println!("agentd tui: not yet implemented")` stub)
- New `agentd` and `agentd-tui` deps: `ratatui`, `crossterm`, `insta` (dev)

### Out of scope (deferred to follow-up)

- Agent picker dropdown inside the new modal (spec §6 says "Agent dropdown → default_agent pre-selected, ↓ cycles plugins"). v1 uses a single default agent.
- `agentd tui --all` flag (tombstones view). TUI shows only non-finished sessions; --all deferred.
- `agentd tui` invoked from a tmux popup. Triggered manually for now.
- §7 ANSI escape for the tmux status line (already done in Plan 2's `status` subcommand — verify and keep as-is).
- Per-agent plugin color customization. All sessions get the same color palette based on status only.

## Design

### 1. Multiplexed JSON-RPC over the control UDS (per spec)

The TUI opens a single UDS connection. Over that one connection:
- TUI → daemon: regular requests (with `id` field, response expected): `state.snapshot`, `session.create`, `session.jump`, `session.kill`, `session.rename`, `daemon.shutdown`, `subscribe`, `unsubscribe`.
- Daemon → TUI: regular responses (matching the `id`).
- Daemon → TUI: notifications (no `id` field): `event` (the bus-broadcast event).

The TUI's `ControlClient` gets a `subscribe()` method that:
1. Sends `subscribe` request with a filter like `{"events": ["session.*", "plugin.*"]}`.
2. Spawns a background reader task that reads frames from the UDS, dispatches to either the request/response waiter (if frame has `id`) or the event handler (if frame has no `id`).
3. Returns a `broadcast::Receiver<Event>` that the TUI subscribes to.

The TUI never polls; events arrive as they happen.

### 2. Daemon-side subscriber list

A new `SubscriberRegistry` in `crates/agentd/src/handlers/subscriber_registry.rs`:
- `Arc<Mutex<HashMap<ConnectionId, mpsc::Sender<Event>>>>` (or a `Vec<UnboundedSender<Event>>` if we don't need per-connection identity).
- `register(tx) -> ConnectionId`, `unregister(id)`, `broadcast(event)`.
- Created at daemon boot, shared with the control UDS handler.

The daemon's `EventBus` is updated so that `emit()` also calls `SubscriberRegistry::broadcast(event)` after pushing to the broadcast channel. The broadcast channel and the subscriber registry are independent paths to the same events — TUI uses the registry, anything else can still subscribe via the bus.

Actually, simpler: subscribe the subscriber registry to the bus, and have the registry forward to all its senders. The bus already exists; the registry is a downstream consumer.

```rust
// In Daemon::run:
let bus = self.bus.clone();
let registry = Arc::new(SubscriberRegistry::new());
let mut bus_rx = bus.subscribe();
tokio::spawn(async move {
    while let Ok(event) = bus_rx.recv().await {
        registry.broadcast(&event);
    }
});
```

The control handler gets an `Arc<SubscriberRegistry>` to register on `subscribe` and unregister on connection close (or `unsubscribe`).

### 3. Daemon-side control handler update

The current `handle_client` (in `crates/agentd/src/handlers/router.rs`) is request/response. Add a new arm: `Method::SUBSCRIBE` and `Method::UNSUBSCRIBE`. After handling `subscribe`, the handler transitions to "subscriber mode":

```rust
// In router::handle_client, after dispatching subscribe:
if method == SUBSCRIBE {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let id = registry.register(tx);
    // Send subscribe response (id + result)
    write_frame(&mut stream, &ok_response).await?;
    // Stream events until client disconnects or unsubscribes
    loop {
        tokio::select! {
            event = rx.recv() => {
                let frame = serde_json::json!({"jsonrpc":"2.0","method":"event","params":event});
                if write_frame(&mut stream, &frame).await.is_err() { break; }
            }
            // Also need to read incoming unsubscribe/close — see note below
        }
    }
    registry.unregister(id);
    return Ok(());
}
```

Issue: while in subscriber mode, the handler isn't reading from the stream. The client's unsubscribe/disconnect won't be detected promptly. Fix: `select!` over a `read_frame` future as well, with a 100ms idle timeout — if read returns 0 bytes (EOF), break; if read returns a request (e.g., unsubscribe), process it.

For v1, simpler: just check `stream.take_error()` or use a 0-byte read with a short timeout. The TUI will never send unsubscribe in v1 (it just disconnects on quit), so a 0-byte read is sufficient.

### 4. TUI state struct

```rust
pub struct TuiState {
    pub sessions: Vec<Session>,
    pub plugins: Vec<Plugin>,
    pub selected: usize,           // index in sessions
    pub status_counters: StatusCounters,  // 5 working / 2 waiting / 1 errored / etc.
    pub flash_until: HashMap<Uuid, Instant>,  // session id -> flash end time
    pub dirty: bool,               // any pane needs redraw
    pub show_help: bool,
    pub rename_modal: Option<RenameModal>,
    pub new_modal: Option<NewModal>,
    pub status_message: Option<(String, Instant)>,  // transient status bar text
}
```

`TuiState` is updated by the event handler (which receives the bus events) and the input handler (which updates `selected` etc.). Render reads it.

### 5. Color palette (§7)

`crates/agentd/src/tui/color.rs`:

```rust
pub enum StatusColor { Working, Waiting, Errored, Idle }

pub fn style_for(status: StatusColor) -> Style {
    // ANSI 256 per spec: working=71 (green), waiting=178 (yellow), errored=167 (red), idle=244 (gray)
    match status {
        StatusColor::Working => Style::default().fg(Color::AnsiValue(71)),
        StatusColor::Waiting => Style::default().fg(Color::AnsiValue(178)),
        StatusColor::Errored => Style::default().fg(Color::AnsiValue(167)),
        StatusColor::Idle => Style::default().fg(Color::AnsiValue(244)),
    }
}

pub fn symbol_for(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Working => "●",
        SessionStatus::WaitingForUser => "⚠",
        SessionStatus::Errored => "✕",
        SessionStatus::Idle => "◌",
        SessionStatus::Starting => "◌",
        SessionStatus::Finished => "◌",
    }
}
```

`--no-color` CLI flag disables all colors (falls back to symbols only). `--color=auto` (default) uses `isatty(STDOUT)`.

### 6. 3-pane layout

`crates/agentd/src/tui/render.rs` exposes `render(frame: &mut Frame, state: &TuiState)`. The layout:
- Header (1 row): counters
- Session list (40% of remaining height, or all if no detail)
- Detail (40% of remaining height, hidden if no sessions)
- Footer (1 row): key hints

`ratatui::layout::Layout::vertical([Length(1), Percentage(40), Percentage(40), Length(1)])`.

Each pane has a `render_inner(area, state)` function. The session list uses `ratatui::widgets::List` with `ListState` for selection. The detail pane is a `Paragraph` with the selected session's fields.

### 7. Event loop

```rust
// crates/agentd/src/tui/mod.rs
pub async fn run() -> Result<()> {
    let mut client = ControlClient::connect(&paths.control_socket_path).await?;
    let snap = client.call("state.snapshot", json!({})).await?;
    let state = TuiState::from_snapshot(snap);
    let events = client.subscribe(json!({"events": ["session.*", "plugin.*"]})).await?;
    let mut events_rx = events;
    let mut terminal = setup_terminal()?;
    loop {
        // Render
        terminal.draw(|f| render::render(f, &state))?;
        // Wait for either an event or a key
        tokio::select! {
            event = events_rx.recv() => {
                if let Some(event) = event { state.apply_event(event); }
            }
            key = read_key() => {
                if input::handle_key(&mut state, key, &client).await? { break; }
            }
        }
    }
    restore_terminal()?;
    Ok(())
}
```

`read_key` polls `crossterm::event::poll(Duration::from_millis(100))` then `read()`. 100ms = 10fps polling, fine for a 30fps render. (Could increase if needed.)

### 8. Keybindings

| Key | Action |
|---|---|
| `j` / `↓` | select next session |
| `k` / `↑` | select previous |
| `g` | select first |
| `G` | select last |
| `Enter` | jump (session.jump RPC) |
| `r` | open rename modal |
| `c` | open new modal |
| `x` | kill (session.kill RPC + confirmation prompt) |
| `?` | toggle help |
| `q` / `Ctrl-C` | quit |
| `Esc` | close any open modal |

In rename modal: type to edit, Enter to commit (session.rename RPC), Esc to cancel.
In new modal: type to filter recents, Enter to create (session.create RPC), Esc to cancel.

### 9. Dirty row tracking

`TuiState.dirty: bool` is set true on any state change. The render loop only calls `terminal.draw` when `dirty`. The flash animation needs per-frame redraws for 500ms, so `dirty` is also set true while any session is in its flash window.

`terminal.draw` is called with a frame rate cap of 30fps (33ms) — if `dirty` is set and the last draw was <33ms ago, skip.

### 10. New modal (§6 + spec §767)

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
│  Agent: opencode                        │  <- read-only for v1
│                                         │
│  ⏎ create   ⎋ cancel                    │
└─────────────────────────────────────────┘
```

Modal struct holds `query: String`, `recents: Vec<(String, DateTime)>` (loaded at open from `session.list_active` filtered to unique `working_dir` sorted by `last_event_at`). Filtering is naive `starts_with` (no `nucleo` in v1). Agent is hardcoded `opencode` for v1.

### 11. Rename modal

Single text input. On open, pre-fill with the current `display_name`. On commit, `session.rename` RPC with the new name.

### 12. Test strategy (per spec §14)

- `crates/agentd/tests/tui.rs` with `insta` snapshot tests at 80×24 and 200×50.
- Use `ratatui::backend::TestBackend` to render into a buffer, then snapshot the buffer.
- Snapshots: empty, each status, 10 mixed, errored overlay, help modal, new modal (empty/filtering/selected), rename modal.
- The TUI's event loop is hard to integration-test (crossterm + terminal). Most tests are unit tests on `render::*` and `input::*` that take a `&mut TuiState` and a `KeyEvent` and assert state mutations.

## Task list (13 tasks, one commit each)

1. **feat(tui): tui module skeleton + ratatui deps** — Create `crates/agentd/src/tui/` with empty mod files. Add `ratatui`, `crossterm` to `crates/agentd/Cargo.toml`. Wire `agentd tui` subcommand in `main.rs` (replaces the `println!` stub, currently a no-op).
2. **feat(tui): color palette (§7)** — `tui/color.rs` with `style_for`, `symbol_for`, `--no-color` / `--color=auto` detection. Unit tests for each color.
3. **feat(tui): TuiState struct + event apply** — `tui/state.rs` with `TuiState::from_snapshot(snap)` and `apply_event(event)`. Unit tests for both.
4. **feat(tui): daemon SubscriberRegistry + bus forwarding** — `handlers/subscriber_registry.rs` with `register`, `unregister`, `broadcast`. In `Daemon::run`, spawn a task that subscribes to the bus and forwards to the registry. Unit tests.
5. **feat(tui): daemon subscribe/unsubscribe handler** — In `handlers/router.rs`, add `subscribe` and `unsubscribe` arms. `subscribe` registers the connection in the registry, sends a success response, then loops writing event notifications. On EOF/close, unregister and return. Unit tests for the registration + unregistration logic.
6. **feat(tui): ControlClient subscribe + multiplexed read** — Extend `ControlClient` with `subscribe(filter) -> broadcast::Receiver<Event>`. Background reader task: for each frame, dispatch to request/response waiter (if `id`) or event bus (if no `id`). Unit tests with a fake UDS.
7. **feat(tui): 3-pane render** — `tui/render.rs` with `render(frame, state)`. Header (counters), session list (with selection), detail (selected session), footer (key hints). Snapshot tests at 80×24 and 200×50 for: empty, 1 session, 10 mixed, errored overlay.
8. **feat(tui): input handler + navigation keys** — `tui/input.rs` with `handle_key(&mut state, key, &client)`. Implements j/k/g/G/Enter/x/?/q. Each action returns a "should-quit" bool. Unit tests for each key.
9. **feat(tui): Enter + x action wiring** — `handle_key` for `Enter` (session.jump RPC + status message), `x` (session.kill RPC + confirmation prompt, then commit). Integration test with a stub ControlClient.
10. **feat(tui): rename modal** — `tui/rename_modal.rs` with the modal struct + render. Pressing `r` opens it; Enter commits via `session.rename`; Esc closes. Snapshot test for the open state and the typed state.
11. **feat(tui): new modal** — `tui/new_modal.rs` with the modal struct + recents query (loaded at open from `session.list_active` aggregated by `working_dir`) + render. Pressing `c` opens; type filters; Enter commits via `session.create`; Esc closes. Snapshot tests for empty/filtering/selected states.
12. **feat(tui): help modal + status bar + dirty tracking** — `?` toggles a help overlay (lists all keybindings). Status bar shows transient messages (e.g., "Killed session XYZ" for 3s). Dirty tracking: only redraw when state changes or flash active. Snapshot test for help + status bar.
13. **feat(tui): event loop + agentd tui command** — `tui/mod.rs` `run()` ties everything together: connect, subscribe, event loop, graceful shutdown. Replace the `agentd tui` stub in `main.rs`. Smoke test that the command runs and exits on `q` (with a fake `ControlClient`).

## Dependencies (added to `crates/agentd/Cargo.toml`)

```toml
ratatui = "0.29"
crossterm = "0.28"
insta = { version = "1", dev = true }  # for snapshot tests
```

No new deps in `agentd-protocol` (events are JSON values; no need for new types).

## What NOT to do

- Don't add an `agentd-tui` separate crate. The TUI is a subcommand of `agentd` per spec.
- Don't add per-agent color customization.
- Don't add a nucleo/dialoguer dependency for the new modal. Naive `starts_with` is enough for v1.
- Don't add an `agentd tui --all` flag. Tombstones view is post-v1.
- Don't break `PROTOCOL_VERSION` compat. The new `subscribe` / `unsubscribe` / `event` methods are additive; old clients ignore unknown methods.
- Don't use `colored` crate. Use hand-rolled ANSI for the status line and ratatui's `Color` for the TUI.
- Don't shell out to `tmux` from the TUI. All tmux work goes through the control UDS (session.jump).

## Risks

- **Multiplexed JSON-RPC over a single UDS** is the highest-risk design decision. If the TUI's reader task hangs, the whole TUI hangs. Mitigate: the TUI runs the reader in a `tokio::spawn` so it can be cancelled; cancellation propagates via the connection's `Drop`.
- **Subscriber registry lifetime**: if the registry holds senders for connections that are gone, `broadcast` returns Err on send. We filter out dead senders per `broadcast` call.
- **30fps render with dirty tracking**: ratatui's `Terminal::draw` is sync; we wrap the `select!` in a loop with a frame-rate cap. If `crossterm::event::poll` is slow, render rate drops. Acceptable for v1.
- **Daemon-side `subscribe` handler runs forever**: need explicit timeout or shutdown signal. v1: relies on the client's connection close; if the client crashes, the OS cleans up the UDS, the connection's `read` returns 0, the handler exits.

## Open questions (defer to spec review)

- Should the `subscribe` filter be an exact match (`"session.created"`) or a prefix (`"session."`)? The spec says `events: ["session.*", "plugin.*"]` — that's a glob. v1 implementation: prefix match; treat `"*"` as wildcard. No glob crate.
- The `rename` modal's "commit" semantics: should it require a confirmation (e.g., `y` to confirm), or just commit on Enter? v1: commit on Enter; spec doesn't mention confirmation. Add `y/n` prompt if user feedback wants it.
- The new modal's "agent dropdown": spec says "↓ cycles plugins". v1 has no dropdown (hardcoded opencode). Plan 4.5 can add it.

## Out of scope (v2+)

- TUI: per-agent color customization, per-pane focus indicators, multi-pane split (Vim-style), `/`-search within session list
- TUI: mouse support (crossterm supports it; not in v1)
- TUI: tmux popup integration (`prefix+T` to open the TUI in a popup)
- Daemon: server-pushed `daemon.status` updates on a timer (TUI polls via `state.snapshot` for now)
- Config: TUI keybinding customization
