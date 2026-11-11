---
name: waiting-while-anything-runs-in-the-background
description: "While any background command runs (tests, coverage, a build, a CI watch, a gh sweep) do nothing else; start it as the turn's last call and act only on its notification"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 0253deca-ae3b-48ca-abcc-abf7dc2ebf8f
  modified: 2026-11-11T07:33:19.096Z
---

While any command runs in the background, do nothing but wait. That covers a test or coverage run, a build, a CI watch and a `gh` sweep. Start it as the last tool call of the turn, alone in its message, and do the next piece of work only when its completion notification arrives. Until then: no reading, searching, editing, staging, committing or issue writing, on this issue or any other.

**Why:** The user wants one thing done at a time. Filling the wait makes the session drift: it reads ahead into the next issue or treats a case as passing before the run shows it. Also, cargo and the tests read the working tree while they run, so an edit made mid-run gets tested in place of the state being checked. Adopted from the user's deltahdl repo on 2026-11-11.

**How to apply:**
- If an autopilot reminder fires during the wait, acknowledge it in one line and keep waiting.
- If the user writes during the wait, answer from what is already known. Do the work they ask for after the command finishes, unless they say to do it now.
- An in-progress CI run of any workflow counts. If a newer push will cancel it, wait for the new run.
- Never leave two commands running side by side. When a step needs two results, put both in one background command.
- See [[long-running-commands-in-the-background]].
