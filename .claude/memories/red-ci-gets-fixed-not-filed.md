---
name: red-ci-gets-fixed-not-filed
description: "A red CI job on main is fixed in the same session, whatever the cause (toolchain, tool bug, code); filing an issue is never the stopping point"
metadata:
  node_type: memory
  type: feedback
  originSessionId: b4540918-31f0-41a7-bc3c-2bda70a3487d
  modified: 2026-11-13T23:20:12.421Z
---

# Red CI gets fixed, not filed

When a CI job on main is red, keep working until it is green. That applies even when the cause lies outside the changed code, such as a new toolchain release, a bug in a CI tool, or a gap in files that weren't touched. An issue can track the work, but filing it does not finish the work, and a red run is never something to hand back to the user.

**Why:** main must stay green, and every later change is judged against it. A failure that looks external usually has a workaround in the workflow that can be found and applied right away.

**How to apply:**

- Reproduce the failure from the run's own artifacts before theorizing. The coverage gate's instrumented archive and profraw files can be downloaded with `gh run download`, and the local `llvm-cov` reads Linux ELF binaries.
- Search the failing tool's upstream issues for the symptom, then work around it in the workflow until upstream ships a fix.
- Watch the run after the push, per [Checking locally, then verifying through CI](verifying-through-ci.md), and keep going until it is green.
