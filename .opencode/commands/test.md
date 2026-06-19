# /test

Run tests for the current crate (or the whole workspace).

```bash
cargo nextest run -p $(basename $(pwd)) --no-fail-fast
```

Or for the whole workspace:

```bash
cargo nextest run --workspace --no-fail-fast
```
