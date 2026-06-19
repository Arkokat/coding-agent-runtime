# 0003. MIT license

**Date:** 2026-06-18
**Status:** Accepted

## Context

agentd will be open source. License choice affects:
- Who can use it (everyone, anyone, with conditions)
- What derivative works must do (open-sourced? attributed? patent grant?)
- How familiar contributors are with the license

Options considered: MIT, Apache-2.0, MPL-2.0, AGPL-3.0.

## Decision

**MIT license.** Copyright (c) 2026 arkokat.

## Consequences

Enables:
- Anyone can use, modify, redistribute (commercial or not)
- Plugins can be any license, including closed-source
- Shortest, most familiar license text
- No contributor license agreement (CLA) required

Costs:
- No patent grant (acceptable for a local CLI tool wrapping other CLIs)
- No copyleft protection (anyone can fork without contributing back)
- If we later want patent protection, we'd need to dual-license or switch to Apache-2.0

## Why not Apache-2.0

Apache-2.0 adds an explicit patent grant and is used by major Rust projects (tokio, serde). We chose MIT for simplicity and the assumption that a local CLI tool has no meaningful patent exposure. Easy to switch to dual MIT/Apache-2.0 in the future if needed.

## Why not GPL/AGPL

Viral clauses conflict with the "plugins can be any license" model. AGPL is hostile to a future hosted-service mode. We're not building infrastructure that depends on contribution flow.
