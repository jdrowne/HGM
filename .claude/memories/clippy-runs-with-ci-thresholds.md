---
name: clippy-runs-with-ci-thresholds
description: "local clippy must set CLIPPY_CONF_DIR to etc/clippy/src and etc/clippy/test, and the one-assert check runs with uvx"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 072b30f0-2c5a-422e-8f53-3c9978cc8d7b
  modified: 2026-11-11T16:57:27.594Z
---

# Clippy runs with the CI thresholds

Plain `cargo clippy` uses Clippy's default limits, so a function with six arguments passes locally and fails CI. Run it the way release.yml does:

- `CLIPPY_CONF_DIR=$PWD/etc/clippy/src cargo clippy --workspace --lib --bins -- -D warnings`
- `CLIPPY_CONF_DIR=$PWD/etc/clippy/test cargo clippy --workspace --tests -- -D warnings`

`uvx assert-one-assert-per-test src test` runs the one-assert check locally. It counts `&&` inside an assert as two assertions.

**Why:** the src limit is five arguments including `self`, and src and test have different line limits. Neither shows up without the config folder.

**How to apply:** use these commands for the local fmt and clippy pass described in [Checking locally, then verifying through CI](verifying-through-ci.md).
