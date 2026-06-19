# Release workflow

Maintainers only. Triggered by pushing a `v*` tag.

## Steps (automated by `.github/workflows/release.yml`)

1. **Pre-tag**: ensure `main` is green on all 4 OS/arch targets, all tests pass
2. **Bump version**:
   ```bash
   cargo xtask release patch   # 0.1.0 -> 0.1.1
   cargo xtask release minor   # 0.1.0 -> 0.2.0
   cargo xtask release major   # 0.1.0 -> 1.0.0
   ```
   This updates workspace `Cargo.toml`, all member `Cargo.toml` files (if using workspace version), and creates a `CHANGELOG.md` entry.
3. **Commit + tag**:
   ```bash
   git add .
   git commit -m "chore(release): v0.1.1"
   git tag v0.1.1
   git push origin main v0.1.1
   ```
4. **CI runs release workflow**:
   - Build all 4 targets (linux/macos × amd64/aarch64)
   - Generate SHA256SUMS
   - Upload to GitHub release
   - Publish crates in dependency order to crates.io
   - Update Homebrew formula in `arkokat/homebrew-agentd` (auto-PR)
   - Update `agentd.dev/install.sh` to point at new version
5. **Post-release**: announce in Discussions, update README badges if needed

## Rollback

If a release is broken:

```bash
# 1. Mark the release as pre-release on GitHub
# 2. Yank affected crates: cargo yank --version 0.1.1 agentd-protocol
# 3. Publish a patch: 0.1.2 with the fix
```

Yanking prevents new dependents but doesn't break existing builds. The patch is the actual fix.
