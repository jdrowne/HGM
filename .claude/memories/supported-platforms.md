---
name: supported-platforms
description: RustyHGM supports Linux x86_64 and arm64, macOS arm64, and Windows x86_64 and arm64; CI builds and unit-tests all five
metadata:
  type: project
---

# Supported platforms

RustyHGM supports exactly these hosts: Linux x86_64 and arm64, macOS arm64, and Windows x86_64 and arm64.

**Why:** These are the hosts the user supports. Windows arm64 is included because GitHub offers a native runner for it free of charge: the repo is public, so `windows-11-arm` and `ubuntu-24.04-arm` cost nothing.

**How to apply:** `release.yml` builds and unit-tests on `ubuntu-latest`, `ubuntu-24.04-arm`, `macos-latest`, `windows-latest` and `windows-11-arm`. The integration and e2e tiers drive Linux containers through the Docker daemon, which only the Linux runners provide, so they run on the two Linux platforms. `.gitattributes` checks every file out with LF, because Windows runners otherwise convert to CRLF and byte-exact fixtures fail. Host code must not assume a Unix shell or `/` paths on the host side. Paths inside containers are always Linux paths. Related: [[ci-follows-10u-labs-conventions]].
