# Contributing

## Setup

1. Clone the repo
2. Install Rust 1.85+ via [rustup](https://rustup.rs)
3. `cargo build --workspace` to verify
4. `cargo nextest run --workspace` to run all tests

## Workflow

1. Pick a task from the implementation plan in `docs/superpowers/plans/`
2. Read the relevant spec section in `docs/superpowers/specs/`
3. Read the relevant ADR in `docs/decisions/` if changing a foundational decision
4. TDD: write failing test, see it fail, implement, see it pass, commit
5. Open a PR against `main`
6. CI must pass: fmt, clippy, tests on all 4 OS/arch targets

## PR checklist

- [ ] Tests added/updated
- [ ] `cargo fmt` clean
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo nextest run --workspace` all green
- [ ] `cargo doc --no-deps` no warnings
- [ ] Spec section referenced in commit body if applicable
- [ ] ADR added if changing a foundational decision

## Commit format

Conventional Commits. See `AGENTS.md` for examples and `docs/workflows/commit.md` for details.

## Style

- Rust 2024 edition, MSRV 1.85
- `rustfmt` defaults
- `clippy::pedantic` warnings are fine, not denied
- No `unsafe` in `agentd-protocol`
- Public APIs documented

## Release

Maintainers run `cargo xtask release` which bumps versions, generates a changelog, builds release tarballs for 4 targets, publishes to crates.io, updates the Homebrew tap, and creates a GitHub release. See `docs/workflows/release.md`.
