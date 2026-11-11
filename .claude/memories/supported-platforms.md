---
name: supported-platforms
description: RustyHGM is built, tested and released for Linux x86_64 and arm64 only; no macOS or Windows legs in CI and no such files in a release
metadata:
  type: project
---

# Supported platforms

RustyHGM is built, tested and released for Linux x86_64 and arm64 only.

**Why:** Every run drives Linux containers through Docker, and GitHub's free runners offer that on Linux alone. A macOS or Windows build could never be run end to end in CI, so it is not built or released.

**How to apply:** Every matrix in `release.yml` holds `linux-x86-64` on `ubuntu-latest` and `linux-arm64` on `ubuntu-24.04-arm`, and all three test tiers run in full on both. Do not add macOS or Windows legs, release files or README download rows. Paths inside containers are always Linux paths. Related: [[ci-follows-10u-labs-conventions]].
