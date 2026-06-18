# xtask

Build automation binary. Internal, not published. Not in user-facing distribution.

## Purpose
Single binary that wraps multi-step build operations (fmt + clippy + test, version bump, release tarball generation). Avoids shell scripts.

## Conventions
- Subcommand per operation: `xtask fmt`, `xtask clippy`, `xtask test`, `xtask ci`, `xtask release`
- No external dependencies beyond what `cargo` already provides
- Fast: should never block CI for more than a few seconds
- TDD: every subcommand has a test that runs it against a fixture workspace
