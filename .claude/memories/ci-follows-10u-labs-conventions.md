---
name: ci-follows-10u-labs-conventions
description: CI copies deltahdl and the 10U-Labs repos; look there before inventing a pattern; one workflow per thing it guards; fix code instead of raising a limit
metadata:
  node_type: memory
  type: feedback
  originSessionId: b9d8f025-c532-4c59-ba8d-2b6ada878b5a
  modified: 2026-11-11T18:20:14.594Z
---

# CI follows the 10U-Labs conventions

CI is written the way deltahdl (`../../deltahdl/deltahdl`) and the 10U-Labs repos (`../../10U-Labs/*`) write theirs. What the workflows currently do is read from `.github/workflows/`, not from this note.

**Why:** the 10U-Labs repos are the reference for how workflows are split and named, and deltahdl is the reference for how a compiled language splits its builds, tests and coverage.

**How to apply:**

- Before adding or changing a CI pattern, look at how those repos do it and copy it.
- A new kind of file gets its own workflow named after what it guards, rather than a job in `release.yml`.
- Fix the code rather than raise a threshold. The one deliberate difference from deltahdl is the 1000-line file limit instead of 949.
- Coverage runs on stable Rust only, never nightly, even for branch coverage ([Always use the latest versions](always-use-latest-versions.md)). New code lands with its tests at full coverage.

Related: [Supported platforms](supported-platforms.md), [No comments in any file](no-comments-in-any-file.md), [Checking locally, then verifying through CI](verifying-through-ci.md).
