# agentd-testing

Test harness, fixtures, and HTTP mock for agentd.

## Public API
- `Harness` — temp dir with XDG layout, cleanup on drop
- `test_agent` binary (built from `src/bin/test_agent.rs`) — reads script, emits events
- `HttpMock` — axum server with canned scenario responses
- `ScriptedSession` — static factory methods for common session flows
- `AgentEnv` — per-agent base URL helpers (env vars or CLI args)

## Constraints
- Tests are real, no mocks for plugin logic
- HTTP mock is at the wire level — plugin code is unmodified
- Every helper has a unit test
- Every scenario fixture in `fixtures/http/<agent>/scenarios/*.toml` is exercised by at least one test

## What NOT to do
- Don't add `unsafe`
- Don't add I/O outside the harness boundary
- Don't depend on real network in any test
