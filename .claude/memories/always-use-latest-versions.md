---
name: always-use-latest-versions
description: "Never pin versions (toolchains, crates, actions, images); always track latest"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 0253deca-ae3b-48ca-abcc-abf7dc2ebf8f
  modified: 2026-11-11T04:36:35.195Z
---

Do not pin versions anywhere in jdrowne/RustyHGM. Always use the latest.

**Why:** The user said so on 2026-11-10: "do not pin versions. always use `latest`". This overrides the "pin the Rust toolchain" step in issue #12.

**How to apply:**
- Rust toolchain: `channel = "stable"` in `rust-toolchain.toml`, and no `rust-version` MSRV.
- Crate dependencies: `version = "*"`. `Cargo.lock` is gitignored, not committed, and CI does not pass `--locked`.
- GitHub Actions: `@latest` where the action publishes that tag (the 10U-Labs actions do); otherwise its default branch (`@main` or `@master`).
- Docker images: use the `:latest` tag.
- Python interpreters for tooling (such as `uv venv`): the newest available, unless it cannot run the code.

Related: [[commit-directly-to-main]].
