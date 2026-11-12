---
name: machine-and-coding-harness
description: "rustyhgm is \"the machine\"; the program it evolves is \"the coding harness\", never \"agent\" or \"coding agent\", including in prompts the model reads"
metadata:
  node_type: memory
  type: feedback
  originSessionId: f5fbd6fe-aad2-44d4-98e9-a7625b696b3f
  modified: 2026-11-11T21:15:33.426Z
---

# Machine and coding harness

`rustyhgm` is the **machine**. The program it builds and evolves is the **coding harness** (`rustyhgm-coding-harness`, `src/coding-harness/`), in code, flags, folders, messages, tests, README, issues and the prompts sent to the model. "Harness" means only the coding harness; benchmark runners get another name (`Evaluator`). The model is a fixed input, not the thing that improves.

**Why:** "agent" made readers think the language model itself evolves, and "harness" had drifted onto `rustyhgm` and the bench runners too.

**How to apply:** don't write "agent" or "coding agent" for the evolved program, and don't call `rustyhgm` a harness. Exceptions: the paper's title, the original project's file names, `ureq::Agent`, `swebench.harness`, and Claude Code's subagents. The first version is still the progenitor: [The first harness is the progenitor](the-first-harness-is-the-progenitor.md).
