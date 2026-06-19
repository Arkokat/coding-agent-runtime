# agentd

Daemon + CLI binary. Single binary, subcommands via clap derive.

## Public API surface
- Binary `agentd` with subcommands from `cli.rs`
- Crate-internal modules: `cli`, `paths`, `config`, `db`, `ipc`, `handlers`, `tmux`, `event_bus`, `status`, `plugin_supervisor`, `session_create`, `daemon`

## Constraints
- Single binary. No library output.
- No `unsafe` without a clear comment justifying why.
- No `unwrap()` in non-test code.
- Every fallible op has explicit error handling. No silent failures.
- All public types documented with `///` doc comments.
- All `pub` items have a unit test where reasonable (state machines, validators, formatters).
- Status line generation: must NOT touch SQLite. Reads from in-memory cache only.
- Hot path (status call): cold start < 5ms, p99 < 50ms, hard limit 1s.

## Testing
- `cargo test -p agentd`
- Unit tests co-located with code (`#[cfg(test)] mod tests`)
- Integration tests in `crates/agentd/tests/`
- Real-tmux tests gated behind `--features real-tmux` (post-v1)

## What NOT to do
- Don't add I/O to `agentd-protocol`
- Don't break `PROTOCOL_VERSION` compat without bumping it and writing an ADR
- Don't add a network endpoint in v1 (no HTTP scrape, no OTLP push — both v2)
- Don't shell out to `tmux` directly — go through the `Tmux` trait
