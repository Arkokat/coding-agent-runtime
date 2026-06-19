# 0004. Paywall-free e2e testing via HTTP mock

**Date:** 2026-06-18
**Status:** Accepted

## Context

We need e2e tests for all 4 plugins (opencode, Claude Code, Codex, Aider). The most thorough test runs each plugin against its real agent, which costs money:

- opencode → OpenAI-compatible API (cents per test run)
- Claude Code → Anthropic API (cents per test run, more for larger models)
- Codex → OpenAI API (cents)
- Aider → OpenAI-compatible API (cents)

For a project whose maintainer can't pay for CI API calls, paid e2e tests are off the table.

Earlier proposal: recorded-replay (VCR) — record once with a real API, replay forever. But the first record still costs money.

## Decision

**HTTP mock server** in `agentd-testing`, using `axum`. The mock listens on `127.0.0.1:<random port>` and replays canned responses from per-agent scenario fixtures.

All 4 agents support a base-URL env var for routing through a proxy/gateway:

| Agent | Env var | Confirmed |
|---|---|---|
| opencode | `OPENAI_BASE_URL` | yes |
| Claude Code | `ANTHROPIC_BASE_URL` | yes (docs: "Override the API endpoint to route requests through a proxy or gateway") |
| Codex | `OPENAI_BASE_URL` | verify in implementation |
| Aider | `--openai-api-base` | yes |

The plugin runs unmodified, talks to a real local HTTP server, gets real HTTP responses. The responses are scripted.

## Consequences

Enables:
- 100% paywall-free CI
- Hermetic tests (no network)
- Deterministic (same scenario = same output)
- Fast (no real network latency)

Costs:
- Won't catch upstream agent changes (new required header, new response field)
- Won't catch API-side changes (new response format)
- Maintenance: when agents change, update scenario fixtures

## Mitigation for missing coverage

- Maintainer runs plugin against real agent manually before each release
- Check agent `CHANGELOG` for breaking changes
- Update scenarios accordingly
- Opt-in `LIVE_LLM_TESTS=1` repo variable (set by maintainer) enables real-agent smoke, soft-warn only

## Alternatives considered

**VCR recording** — record once with real API, replay. Costs money for first record, replay is free. Rejected because even one-time cost is out of scope.

**Local Ollama for Claude Code** — Anthropic doesn't support routing to non-Anthropic models in stock form. Would need a proxy like `claude-code-router`. Added complexity for marginal benefit. Rejected.

**No e2e tests** — only unit tests with byte fixtures. Misses the entire HTTP handling layer. Rejected.
