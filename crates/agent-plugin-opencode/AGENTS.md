# agentd-plugin-opencode

Reference plugin. Demonstrates how a real `agentd-plugin-*` is
structured: SDK-based transport, NDJSON event source, JSON-RPC
calls to the daemon.

## Public API
- Binary `agentd-plugin-opencode`
- Flags: `--socket <path>` (required), `--mock` (scripted events), `--no-hello` (skip hello)

## Constraints
- Reference only. The real opencode bridge is post-v1.
- Must use `agentd-plugin-sdk` for all daemon communication.
- Must emit `session.discover` for any newly observed tmux pane
  running an opencode process.

## What NOT to do
- Don't reinvent the framing. The SDK does that.
- Don't add an HTTP client to the daemon-side flow. The plugin
  talks to opencode's API; the daemon is unaware.
- Don't shell out to `tmux` directly. Use the daemon's session info.
