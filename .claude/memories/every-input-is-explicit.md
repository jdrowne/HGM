---
name: every-input-is-explicit
description: "rustyhgm and rustyhgm-coding-agent take every value setting as a required flag; yes/no settings are bare switches that are off unless given; no defaults, no environment variables, no working-directory lookups"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 9cd172c9-f7d5-4300-a0ea-24a2132f94e2
  modified: 2026-11-11T20:41:24.592Z
---

# Every input is explicit

Every setting of `rustyhgm` and `rustyhgm-coding-agent` that takes a value is a required command-line flag. No such setting has a default value (no clap `default_value`, no serde `default`, no fallback when a file is missing), no environment variable is read, and nothing is found by looking in the working directory.

Yes/no settings are the one exception: they are bare switches like `--verbose`, on when the flag is typed and off when it is left out (`--cool_down`, `--full_eval`, `--polyglot`, `--self_improve`). They never take `true` or `false`.

**Why:** a run should be fully described by what was typed, so nothing changes behaviour behind the user's back and a downloaded binary works from anywhere. A switch that is off unless typed hides no choice, and it is what people expect of a command-line tool.

**How to apply:** a missing value flag is an error that names it. A new yes/no setting is a plain clap `bool` field with no `action`, `required` or value. API keys come from a file named by a flag, never from the key itself on the command line. Things that are part of the program itself (the progenitor source, the builder Dockerfile, the ignore list) are built into the binary, not read from disk. See [[always-use-latest-versions]] for the same no-implicit-choices stance on tooling.
