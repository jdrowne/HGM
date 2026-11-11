---
name: commit-directly-to-main
description: No feature branches or PRs; commit and push straight to main in jdrowne/RustyHGM
metadata:
  node_type: memory
  type: feedback
  originSessionId: 0253deca-ae3b-48ca-abcc-abf7dc2ebf8f
  modified: 2026-11-11T04:34:53.462Z
---

Do not create branches or open PRs in this repo. Every change is committed directly on `main` and pushed to `origin main`.

**Why:** The user said so on 2026-11-10 ("you should not be using branches. everything should go to main"). This overrides the "series of small PRs" wording in issues #11 and #12.

**How to apply:** Keep the #11 granularity rule (one logical, unsplittable change), but apply it per commit on `main` instead of per PR. Related: [[work-only-on-this-repo]].
