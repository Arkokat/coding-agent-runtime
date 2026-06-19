# xtask

Build automation binary. Internal, not published. Not in user-facing distribution.

## Purpose
Single binary that wraps multi-step build operations (fmt + clippy + test, version bump, release tarball generation). Avoids shell scripts.

## Conventions
- Subcommand per operation: `xtask fmt`, `xtask clippy`, `xtask test`, `xtask ci`, `xtask release` (release is post-v1; currently unimplemented)
- No external dependencies beyond what `cargo` already provides
- Fast: should never block CI for more than a few seconds
- TDD: every subcommand has a smoke test in `tests/` (e.g. `tests/ci_smoke.rs`)
- All four real subcommands (`fmt`, `clippy`, `test`, `ci`) are implemented in `src/cmd.rs` since Plan 2 Task 5. `ci` chains them with a `#[allow(unreachable_code)]` because each subcommand is `-> !` — the chain IS the orchestration.
