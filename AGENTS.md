# AGENTS.md

Instructions for coding agents (opencode, Claude Code, Codex, Aider) working in this repo. Read this first.

## Project

agentd — a Rust daemon that orchestrates coding-agent sessions inside tmux. See `ARCHITECTURE.md` for overview, `docs/superpowers/specs/2026-06-18-agentd-design.md` for the full design.

## Crate layout

- `crates/agentd-protocol` — JSON-RPC 2.0 types, no I/O, no async
- `crates/agentd-testing` — test harness, fixtures, HTTP mock
- `xtask` — build automation

(Future crates added in later plans: `agentd`, `agentd-tui`, `agent-plugin-sdk`, per-agent plugins.)

## Build, test, lint

```bash
cargo build --workspace
cargo test --workspace                    # all tests
cargo test -p agentd-protocol             # specific crate
cargo nextest run --workspace             # faster parallel test runner
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps                       # public API docs
cargo audit                               # security advisories
```

CI: GitHub Actions. PR build runs unit + integration on 4 OS/arch targets. Push to `main` runs the same. Tags trigger release workflow.

### `#[ignore]` convention

A test marked `#[ignore = "needs <feature> (some local sandboxes block it)"]` is one that needs a host capability some developer sandboxes don't allow (e.g. AF_UNIX `connect`). Local `cargo nextest run` skips these; CI runs them via `cargo nextest run --run-ignored all`. Use this for environment-dependent tests, not for slow tests — use `#[ignore = "slow"]` for those and a separate `--profile slow` nextest config. See `docs/superpowers/specs/2026-06-20-ci-only-test-tags-design.md`.

## TDD workflow

**Always write a failing test first.** See `docs/workflows/tdd.md`.

- Red: write a test that captures the new behavior
- Green: minimum code to pass
- Refactor: clean up while tests stay green
- Commit: one task = one commit

## Commit convention

Conventional Commits. Subject ≤50 chars. Body explains WHY, not WHAT. Reference the spec section if applicable.

```
feat(protocol): add SessionStatus enum with serde
fix(testing): handle UDS disconnect race in TestHarness
docs: add ADR-0003 for MIT license
chore(ci): add nightly workflow
test(protocol): roundtrip Session serialization
refactor(protocol): split methods.rs by category
```

Pre-commit runs `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo nextest run`. Skip with `AGENTD_SKIP_HOOKS=1` if you know what you're doing.

## Code style

- Rust 2024 edition, MSRV 1.85
- `rustfmt` defaults
- `clippy::pedantic` is on (warnings are fine, not denied)
- No `unsafe` in `agentd-protocol`. Other crates: comment justifying.
- No `unwrap()` outside `#[cfg(test)]` or `expect("reason")`
- Public APIs have `///` doc comments
- `cargo doc --no-deps` produces zero warnings

## Skills to use

- `brainstorming` — before any creative work, new feature, or behavior change
- `test-driven-development` — for any feature or bugfix, write the failing test first
- `systematic-debugging` — when hitting a bug, before proposing a fix
- `verification-before-completion` — before claiming work is done

## What NOT to do

- Don't add `unsafe` without a clear comment explaining why
- Don't use `unwrap()` in non-test code
- Don't add I/O to `agentd-protocol`
- Don't break the public API of `agentd-protocol` without bumping `PROTOCOL_VERSION` and writing an ADR
- Don't add secrets to git (use env vars, GitHub repo variables)
- Don't make CI depend on paid API keys

## Per-crate guidance

Each crate has its own `AGENTS.md` with crate-specific rules. Read it before working in that crate.

## Subagents

Available in `.opencode/agents/`:

- `explore` — codebase search, returns file:line table, no fixes
- `tdd` — TDD specialist, red-green-refactor cycles
- `reviewer` — code reviewer, severity-tagged findings, no praise
- `planner` — implementation planner, ordered task list

## Decision history

Significant choices are recorded as ADRs in `docs/decisions/`. Read the relevant ADR before changing a foundational decision.
