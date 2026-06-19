# Commit workflow

## Format (Conventional Commits)

```
<type>(<scope>): <subject>

<body>

<footer>
```

- **type**: `feat` | `fix` | `docs` | `style` | `refactor` | `test` | `chore` | `perf` | `build` | `ci`
- **scope** (optional): crate name or area, e.g. `protocol`, `daemon`, `testing`, `tui`, `ci`
- **subject**: ≤50 chars, lowercase, no period, imperative mood
- **body**: explain WHY, not WHAT. Wrap at 72 chars. Reference spec section if applicable.
- **footer**: `BREAKING CHANGE: ...` for breaking changes; `Refs: <spec section>` for traceability

## Examples

```
feat(protocol): add SessionStatus enum with serde

Refs: docs/superpowers/specs/2026-06-18-agentd-design.md section 8
```

```
fix(testing): handle UDS disconnect race in TestHarness

The cleanup on drop could fire before the daemon's accept loop
finished, leaving orphan UDS files in the temp dir. Now we wait
for the daemon's /shutdown RPC before unlinking.
```

```
chore(ci): add nightly workflow
```

## Pre-commit checks (auto-run by git hook)

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo nextest run
```

Skip with `AGENTD_SKIP_HOOKS=1` (don't do this unless you know what you're doing).

## One task = one commit

Each task in the implementation plan is one commit. Commits are atomic: the codebase compiles and tests pass after each commit.

## Force-push / amend policy

- **Amend** is fine for the most recent unpushed commit
- **Force-push** to your own branch is fine
- **Never force-push to `main`**
- **Never amend a pushed commit** (use `git revert` and a new commit instead)
