---
name: commit-directly-to-main
description: No feature branches or PRs; commit and push straight to main in jdrowne/RustyHGM; commits need not be small
metadata:
  node_type: memory
  type: feedback
  originSessionId: 0253deca-ae3b-48ca-abcc-abf7dc2ebf8f
  modified: 2026-11-11T04:35:32.519Z
---

# Commit directly to main

Do not create branches or open PRs in this repo. Every change is committed directly on `main` and pushed to `origin main`. Commits do not need to be small: one commit may cover a whole coherent step, such as a phase checkbox.

**Why:** On 2026-11-10 the user ruled out branches and pull requests, wanted every change on main, and lifted the requirement that commits be small. This overrides the "series of small PRs" wording in issues #11 and #12 and the one-change-per-PR rule in #11.

**How to apply:** Commit coherent units of work on `main` and push them. Don't split work artificially. Related: [[work-only-on-this-repo]].
