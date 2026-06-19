# agentd-plugin-sdk

Helper crate for `agentd-plugin-*` authors. Wraps the daemon's
JSON-RPC protocol and the event source abstraction so plugin code
stays small.

## Public API
- `Backend` trait — `async fn next_event(&mut self) -> Option<Event>`
- `MockBackend` — scripted events for tests
- `RealBackend<R>` — reads NDJSON from any `AsyncBufRead`
- `AgentdClient` — JSON-RPC client for the plugin UDS, with
  typed helpers for `hello`, `discover`, `report_event`, `heartbeat`, `bye`
- `SDK_PROTOCOL_VERSION` — re-export of `agentd_protocol::PROTOCOL_VERSION`

## Constraints
- **No `unsafe`.** Period.
- No business logic. Only transport.
- All plugins should use the same SDK version as the daemon.
- The plugin is responsible for spawning its agent, parsing agent
  output, and calling `AgentdClient::report_event` with normalized
  events. The SDK does no parsing of vendor formats.

## Testing
- `cargo test -p agent-plugin-sdk`
- `MockBackend` roundtrip
- `AgentdClient::connect` errors on missing socket
- `SDK_PROTOCOL_VERSION` matches the daemon's `PROTOCOL_VERSION`
