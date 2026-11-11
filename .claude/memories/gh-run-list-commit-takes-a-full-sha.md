---
name: gh-run-list-commit-takes-a-full-sha
description: gh run list --commit matches only a full 40-character SHA; have the command compute it with git rev-parse, never type or copy a SHA, since a short one lists nothing and a watch loop on it never ends
metadata:
  type: feedback
---

# gh run list --commit takes a full SHA

Find a push's CI run with `gh run list -R jdrowne/RustyHGM --commit "$(git rev-parse HEAD)"`. Never use the short SHA that `git log`, a commit message or the screen shows.

**Why:** `--commit` matches only the full 40-character SHA. A short one lists no runs and raises no error, so a watcher waiting for the list to fill polls forever while its run has long finished. On 2026-11-11 two watchers here looped for about an hour this way. The same rule is in the user's deltahdl repo.

**How to apply:** Wherever a command hands a commit to `gh`, let the command compute the SHA itself, e.g. `sha=$(git rev-parse <ref>) && … --commit "$sha"`. Never write out a SHA, so a copied one can't be wrong. Then watch the run by id with `gh run watch <id> --interval 60 --exit-status` in the background, per [[long-running-commands-in-the-background]].
