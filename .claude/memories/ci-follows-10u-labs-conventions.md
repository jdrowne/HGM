---
name: ci-follows-10u-labs-conventions
description: Workflows follow the 10U-Labs layout; each assert is its own job via its published action; yamllint lints the workflows
metadata:
  type: feedback
---

CI in this repo is written the way the 10U-Labs repos (`../../10U-Labs/*`) and deltahdl write theirs:

- Each check is its own job, named after the tool, with no `needs:` between scanning jobs.
- An assert runs through its published action (`10U-Labs/assert-no-comments@latest`) rather than a hand-written `pip install` step.
- A `yamllint` job lints every workflow file with the 10U-Labs `config_data` (`empty-lines` max 0, `key-ordering`, `truthy` allowing `on`) and `strict: "true"`. So a workflow starts with `---`, has no blank lines, sorts every mapping's keys, and names every step.

**Why:** the user asked for yamllint and assert-no-comments to be adopted "like we do in .. and ../../", so those repos are the reference when a CI question comes up.

**How to apply:** a new workflow file is added to the yamllint job's `file_or_dir`. Before inventing a CI pattern, look at how those repos do it. Related: [[no-comments-in-any-file]], [[always-use-latest-versions]].
