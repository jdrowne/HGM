---
name: always-use-latest-versions
description: "Never pin versions (toolchains, crates, actions, images); always track latest"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 0253deca-ae3b-48ca-abcc-abf7dc2ebf8f
  modified: 2026-11-11T04:36:35.195Z
---

# Always use the latest versions

Do not pin versions anywhere in jdrowne/RustyHGM. Always use the latest.

**Why:** On 2026-11-10 the user ruled out pinning any version and asked for the newest release of everything. This overrides the "pin the Rust toolchain" step in issue #12.

**How to apply:**

- Rust toolchain: `channel = "stable"` in `rust-toolchain.toml`, and no `rust-version` MSRV.
- Crate dependencies: `version = "*"`. `Cargo.lock` is gitignored, not committed, and CI does not pass `--locked`.
- GitHub Actions: `@latest` where the action publishes that tag (the 10U-Labs actions do); otherwise its default branch (`@main` or `@master`).
- Docker images: use the `:latest` tag.
- Python interpreters for tooling (such as `uv venv`): the newest available, unless it cannot run the code.

Related: [[commit-directly-to-main]].

**Stable only.** On 2026-11-11 the user limited the toolchain to stable Rust alone. "Latest" means the latest *stable* Rust, never nightly, even when a feature such as branch coverage exists only on nightly. Do what stable supports, and say what that leaves out.
