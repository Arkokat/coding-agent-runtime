# TDD workflow

**Always write a failing test first.** No exceptions.

## Inner loop (one task)

```bash
# 1. Red — write the failing test
$EDITOR crates/<crate>/tests/<test>.rs

# Run it, see it fail
cargo nextest run -p <crate> -- <test_name>

# 2. Green — write the minimum code to pass
$EDITOR crates/<crate>/src/<module>.rs

# Run it, see it pass
cargo nextest run -p <crate> -- <test_name>

# 3. Refactor — clean up while tests stay green
$EDITOR crates/<crate>/src/<module>.rs
cargo nextest run -p <crate>

# 4. Commit
git add <files>
git commit -m "<conventional commit message>"
```

## Outer loop (one task = one commit)

Each task in the implementation plan is one commit. A task is the smallest unit that:
- Has its own failing test
- Can be reviewed independently
- Leaves the codebase in a compilable state

## Test categories

- **Unit** — pure functions, no I/O, in the same file as `#[cfg(test)] mod tests`
- **Integration** — `tests/` directory, exercises the public API
- **Snapshot** — `insta::assert_snapshot!` for serialized output. Review with `cargo insta review`
- **Property** — `proptest!` for state machines and parsers
- **E2E** — `TestHarness` + real daemon + plugin in mock mode

## What NOT to do

- Don't write the implementation first
- Don't write multiple tests in one commit
- Don't refactor without a test
- Don't commit with failing tests (`cargo nextest run` must be green)
- Don't skip the refactor step (it catches design issues)
