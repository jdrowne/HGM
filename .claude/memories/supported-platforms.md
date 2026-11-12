---
name: supported-platforms
description: "RustyHGM is built, tested and released for Linux x86_64 and arm64 only; no macOS or Windows legs in CI and no such files in a release"
metadata:
  node_type: memory
  type: project
  originSessionId: b9d8f025-c532-4c59-ba8d-2b6ada878b5a
  modified: 2026-11-11T18:20:25.461Z
---

# Supported platforms

RustyHGM is built, tested and released for Linux x86_64 and arm64 only.

**Why:** Every run drives Linux containers through Docker, and GitHub's free runners offer that on Linux alone. A macOS or Windows build could never be run end to end in CI, so it is not built or released.

**How to apply:** All three test tiers run in full on both platforms. Do not add macOS or Windows legs, release files or README download rows. Paths inside containers are always Linux paths. Related: [CI follows the 10U-Labs conventions](ci-follows-10u-labs-conventions.md).
