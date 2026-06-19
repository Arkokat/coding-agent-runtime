# agentd-protocol

The shared vocabulary for agentd IPC. Pure data types, no I/O, no async.

## Public API surface
- Types: `Session`, `SessionStatus`, `SessionEvent`, `EventType`, `Plugin`, `AgentType`
- Error: `ProtocolError` with all JSON-RPC error codes
- Constants: `PROTOCOL_VERSION`, method name strings

## Constraints
- **No `unsafe`.** Period.
- **No async, no I/O.** This crate must be usable from any context.
- **No external dependencies beyond `serde`, `serde_json`, `uuid`, `chrono`, `thiserror`.**
- Every public type has `#[derive(Serialize, Deserialize)]` and roundtrips cleanly.
- Every variant of every enum has a test asserting its string form.

## Testing
- `cargo nextest run -p agentd-protocol` — all tests must pass in <1s
- Coverage target: 95%+
- Snapshot tests for type serialization (insta) — committed to repo
