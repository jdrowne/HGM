---
name: work-only-on-this-repo
description: Only act on jdrowne/RustyHGM; never touch upstream metauto-ai/HGM or any other repo
metadata:
  type: feedback
---

# Work only on this repo

Work only on this repo (jdrowne/RustyHGM, the `origin` remote). Do not touch "the other repo": no issues, PRs, comments, pushes or other writes to the upstream metauto-ai/HGM, and no changes to any other local repo.

**Why:** On 2026-11-10 the user said "we should not touch the other repo. we should work on this repo only". This followed my pointing out that gh's default repo for this clone was the `upstream` remote (metauto-ai/HGM), so bare `gh` commands such as `gh pr create` would have targeted it. The user did not say which repo "the other repo" meant; reading it as upstream HGM, and by extension any repo other than this one, is my interpretation.

**How to apply:** Before any `gh` write (issue, PR, comment), confirm it targets jdrowne/RustyHGM: pass `-R jdrowne/RustyHGM` or check `gh repo view --json nameWithOwner`. Never push to the `upstream` remote. gh's default repo was set to jdrowne/RustyHGM on 2026-11-10; verify it still is rather than assuming.
