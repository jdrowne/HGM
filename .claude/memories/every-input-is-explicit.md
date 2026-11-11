---
name: every-input-is-explicit
description: "rustyhgm and rustyhgm-coding-agent take every setting explicitly; no defaults, no environment variables, no lookups relative to the working directory"
metadata:
  node_type: memory
  type: feedback
  originSessionId: e1d06916-0e12-4044-a8b1-d481032b3ce2
  modified: 2026-11-11T16:30:25.718Z
---

# Every input is explicit

Every input to `rustyhgm` and `rustyhgm-coding-agent` is given explicitly by the person running it. No setting has a default value (no clap `default_value`, no serde `default`, no fallback when a file is missing), no environment variable is read, and nothing is found by looking in the working directory.

**Why:** a run should be fully described by what was typed, so nothing changes behaviour behind the user's back and a downloaded binary works from anywhere.

**How to apply:** every setting is a required command-line flag; there is no config file and no layer that overrides another. A missing flag is an error that names it. API keys come from a file named by a flag, never from the key itself on the command line. Things that are part of the program itself (the seed agent source, the builder Dockerfile, the ignore list) are built into the binary, not read from disk. Do not add a default when adding a new setting. See [[always-use-latest-versions]] for the same no-implicit-choices stance on tooling.
