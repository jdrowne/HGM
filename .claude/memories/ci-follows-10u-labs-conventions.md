---
name: ci-follows-10u-labs-conventions
description: One workflow per thing it guards (release.yml for the compiled product, markdownlint.yml); build, unit, integration and e2e chained through a build artifact, as deltahdl does
metadata:
  type: feedback
---

# CI follows the 10U-Labs conventions

CI is written the way deltahdl (`../../deltahdl/deltahdl`) and the 10U-Labs repos (`../../10U-Labs/*`) write theirs:

- One workflow per thing it guards, named after it: `release.yml` for the compiled Rust product, as the 10U-Labs tools name theirs (deltahdl calls its own `deltahdl.yml`), and `markdownlint.yml` for the Markdown. Each starts on a push to `main` filtered by `paths`, and on `workflow_dispatch`, with a `concurrency` group that cancels older runs.
- Every workflow checks itself: its own `assert-no-comments` and `yamllint` jobs cover the workflow file, and `yamllint` also runs `assert-no-inline-directives --tools yamllint`.
- Because Rust is compiled, the product workflow builds once per platform and the test tiers consume that build, as deltahdl does with CMake: `build-<platform>` writes a `cargo nextest archive` and uploads it, and `unit-test-<platform>`, `integration-test-<platform>` and `e2e-test-<platform>` each need `build` alone, so the tiers run side by side (the user asked on 2026-11-11 that integration and e2e need only `build`). Lint checks (`clippy`, `rustfmt`, the asserts) are their own jobs with no `needs:`.
- An assert with a published action runs through it (`10U-Labs/assert-no-comments@latest`). `yamllint` uses the 10U-Labs rules (`empty-lines` max 0, `key-ordering`, `line-length` disabled, `new-line-at-end-of-file`, `truthy` allowing `on`) with `--strict`, so a workflow starts with `---`, has no blank lines, sorts every mapping's keys, and names every step.

**Why:** the user asked on 2026-11-11 for at least two workflows like the 10U-Labs `release.yml` and `markdownlint.yml`, and pointed at deltahdl for how a compiled language splits its build from its tests. The product workflow was first named `rustyhgm.yml` after deltahdl's; the user had it renamed to `release.yml` the same day.

**How to apply:** a new kind of file gets its own workflow rather than a job in `release.yml`. Before inventing a CI pattern, look at how those repos do it. Related: [[supported-platforms]], [[no-comments-in-any-file]], [[always-use-latest-versions]].
