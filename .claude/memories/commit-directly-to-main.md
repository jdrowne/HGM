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

**Why:** The user wants every change on main, without branches, pull requests or a size limit on commits. Where issues #11 and #12 speak of a series of small PRs or one change per PR, this rule wins.

**How to apply:** Commit coherent units of work on `main` and push them. Don't split work artificially. Related: [[work-only-on-this-repo]].
