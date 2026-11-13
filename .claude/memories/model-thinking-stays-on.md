---
name: model-thinking-stays-on
description: "never propose turning off a thinking model's reasoning to save time; bound reply length or time instead"
metadata:
  node_type: memory
  type: feedback
  originSessionId: b4540918-31f0-41a7-bc3c-2bda70a3487d
  modified: 2026-11-13T13:13:58.005Z
---

# Model thinking stays on

Thinking stays enabled for the models rustyhgm and the coding harness use. Do not propose `reasoning_effort: "none"`, a think-off switch or any other way of disabling it, not even as a left-out option.

**Why:** reasoning is part of how the model is meant to work, and slowness is a problem to solve without giving it up.

**How to apply:** when replies run long, bound tokens per reply, time per request or retries, and handle cut-off replies; keep thinking on. Related: [Machine and coding harness](machine-and-coding-harness.md).
