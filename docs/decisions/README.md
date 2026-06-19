# Architecture Decision Records (ADRs)

Significant design decisions, one per file. Numbered sequentially. New ADRs go at the next number.

## Format

```markdown
# NNNN. Title

**Date:** YYYY-MM-DD
**Status:** Accepted | Superseded by NNNN
**Context:** What was the situation
**Decision:** What we chose
**Consequences:** Trade-offs, what this enables, what it rules out
```

## Index

- [0001](0001-json-rpc-over-uds.md) — JSON-RPC 2.0 over Unix domain sockets
- [0002](0002-tmux-interface.md) — Use `tmux_interface` crate for tmux control
- [0003](0003-mit-license.md) — MIT license
- [0004](0004-paywall-free-testing.md) — Paywall-free e2e testing via HTTP mock
