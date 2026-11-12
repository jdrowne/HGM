---
name: verifying-through-ci
description: "Locally, run only what proves a change works (its new or touched tests, fmt/clippy/no-comments on what changed); workspace-wide test runs, coverage, Docker tiers and other platforms are CI's verdict, read after the push"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 0253deca-ae3b-48ca-abcc-abf7dc2ebf8f
  modified: 2026-11-11T07:35:48.866Z
---

# Checking locally, then verifying through CI

Run a change locally only far enough to show it works. That means the tests it adds or changes, filtered with `cargo nextest run -p <crate> -E 'test(...)'`, plus `cargo fmt --all --check`, clippy on the touched crates (both `etc/clippy/src` and `etc/clippy/test` configs) and the no-comments check. Everything that runs across the whole workspace belongs to CI (`release.yml`):

- `cargo llvm-cov nextest --workspace`, including the 100% coverage gate;
- the Docker integration and e2e tiers;
- the per-platform builds;
- the test-file registration, line-limit and lint-config checks.

Push, then read the run for the verdict.

**Why:** A local run earns its time only by answering whether a candidate change works before it is settled on. A full workspace sweep with coverage and Docker tiers takes many minutes and tells nothing one CI run does not.

**How to apply:**

- Build and run each new test locally and see it pass, and fail first where the change fixes a bug. Do the same for the existing tests of the code in hand.
- Don't run the whole workspace under `cargo llvm-cov` locally. Find uncovered lines from CI's `coverage-html` artifact and the gate's output, then fix them with targeted tests.
- Integration and e2e tests that need Docker may be run locally when they are the tests a change adds, filtered to those tests. The full tiers are CI's.
- Local commands that may run past a minute still go in the background, per [Long-running commands in the background](long-running-commands-in-the-background.md).
