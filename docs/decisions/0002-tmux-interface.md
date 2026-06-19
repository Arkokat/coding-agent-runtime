# 0002. Use `tmux_interface` crate for tmux control

**Date:** 2026-06-18
**Status:** Accepted

## Context

agentd needs to control tmux: create sessions, switch clients, list panes, send keys, capture output. Options:

1. **Subprocess to `tmux` CLI** with hand-rolled command builders
2. **Hand-roll `libtmux`-style bindings** (Rust equivalent of Python `libtmux`)
3. **Use the `tmux_interface` crate** (v0.4, 19K downloads, wraps `tmux` CLI with typed builders)

## Decision

Use the `tmux_interface` crate with the `tmux_3_3` feature (matches our minimum supported tmux version).

## Consequences

Enables:
- Typed builders: `NewSession::new().detached().session_name(...)`
- Feature flags for tmux version compatibility
- 19K downloads = battle-tested
- No hand-rolled command parsing

Costs:
- Adds a dependency
- Versions below 1.0 may have API churn (we'll pin to a specific minor)
- We don't get a programmatic API (tmux control mode is a draft in this crate); we use the CLI subprocess under the hood

## Alternatives considered

**Direct `std::process::Command` calls to `tmux`** — more code, no type safety, easier to introduce shell injection bugs. Rejected.

**Control mode** — long-lived tmux connection, push events. Saves polling but adds state machine complexity. Post-v1 consideration.
