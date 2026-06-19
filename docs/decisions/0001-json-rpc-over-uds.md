# 0001. JSON-RPC 2.0 over Unix domain sockets

**Date:** 2026-06-18
**Status:** Accepted

## Context

agentd has three IPC surfaces: CLI/TUI → daemon (control), plugin → daemon (plugin), and daemon → subscribers (events). The IPC must be:
- Fast (sub-ms for status call)
- Type-safe (don't accept arbitrary blobs)
- Debuggable (must be inspectable with `socat` or `nc`)
- Local-only (single user, single machine)
- Standard (so plugin authors in any language can implement it)

## Decision

Use **JSON-RPC 2.0** over **Unix domain sockets** (UDS) with **NDJSON framing** (one JSON object per line).

For the type system, share a Rust crate (`agentd-protocol`) between daemon and all plugins. Plugins in non-Rust languages can hand-roll the types from the JSON Schema.

## Consequences

Enables:
- Tooling: `socat - UNIX-CONNECT:...` for ad-hoc debugging
- Multi-language plugins: any language with a JSON-RPC library works
- Type-safe Rust ecosystem: no codegen, no protobuf compiler
- Standard error codes: -32700..-32005 mapped to our domain

Costs:
- JSON is slower than binary formats (acceptable for sub-ms targets)
- NDJSON breaks if any string contains a raw newline (we use `serde_json::to_string` which never produces raw newlines)
- No streaming by default (future: add `Content-Type: text/event-stream` for SSE-style events)
- Authentication is uid-based (single-user assumption)
