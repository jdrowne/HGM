---
name: long-running-commands-in-the-background
description: "Any command that may run past about a minute (cargo test/nextest/llvm-cov/clippy, Docker builds, gh sweeps, CI watches) goes in a background Bash call and ends the turn; never wait in the foreground; gh run watch takes --interval 60"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 0253deca-ae3b-48ca-abcc-abf7dc2ebf8f
  modified: 2026-11-11T07:33:14.149Z
---

Run every command that can take more than about a minute with `run_in_background: true`, then end the turn and pick the work back up from its completion notification. In this repo that means workspace test runs (`cargo nextest`, `cargo llvm-cov nextest`), clippy over the workspace, Docker image builds, integration and e2e tiers, sweeps of `gh` calls, and every wait on a CI run.

**Why:** The autopilot reminders are cron jobs, and a cron job fires only while the session sits idle between turns. A foreground command holds the turn, so reminders queue behind it and the user cannot type. A background command frees the turn at once. Adopted from the user's deltahdl repo on 2026-11-11.

**How to apply:**
- Write the output to a file in the scratchpad and read it once the notification arrives.
- Bound any command that might hang: macOS has no `timeout`, so use `perl -e 'alarm N; exec @ARGV' <cmd>`.
- Never follow a background command with a foreground wait. That means no `sleep`, no `until` loop, and no polling its output file.
- Watch CI with `gh run watch <id> --interval 60 --exit-status` in the background, or with Monitor. The default 3-second poll trips GitHub's secondary rate limit, and that limit clears only after minutes of quiet.
- A turn made of many short foreground calls also holds reminders back, so split long work at natural boundaries, such as after a batch of edits.
- While it runs, follow [[waiting-while-anything-runs-in-the-background]].
