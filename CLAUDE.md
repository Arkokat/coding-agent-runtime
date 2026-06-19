# CLAUDE.md

Quick reference for Claude Code. The full agent instructions are in `AGENTS.md` — read both.

## Project

agentd — Rust daemon for tmux + coding-agent orchestration. See `AGENTS.md` for the full picture.

## Build & test

```bash
cargo build --workspace
cargo nextest run --workspace
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## TDD

Always write the failing test first. See `docs/workflows/tdd.md`.

## When in doubt

- Spec: `docs/superpowers/specs/2026-06-18-agentd-design.md`
- ADRs: `docs/decisions/`
- Workflows: `docs/workflows/`

## Subagents

This repo defines subagents for opencode (`.opencode/agents/`). For Claude Code, use the Task tool with the `general-purpose` agent for codebase exploration.

## Constraints

Same as `AGENTS.md`: no `unsafe` in protocol, no `unwrap()` in non-test, TDD first.
