---
name: work-only-on-this-repo
description: Only act on jdrowne/RustyHGM; never touch upstream metauto-ai/HGM or any other repo
metadata:
  type: feedback
---

# Work only on this repo

Work only on this repo (jdrowne/RustyHGM, the `origin` remote). Do not touch the upstream repo: no issues, PRs, comments, pushes or other writes to the upstream metauto-ai/HGM, and no changes to any other local repo.

**Why:** The user keeps all work in this repo, and upstream metauto-ai/HGM is off limits. This clone has an `upstream` remote, so a bare `gh` command can land on the wrong repo.

**How to apply:** Before any `gh` write (issue, PR, comment), confirm it targets jdrowne/RustyHGM: pass `-R jdrowne/RustyHGM` or check `gh repo view --json nameWithOwner`. Never push to the `upstream` remote. gh's default repo should be jdrowne/RustyHGM; verify it rather than assuming.
