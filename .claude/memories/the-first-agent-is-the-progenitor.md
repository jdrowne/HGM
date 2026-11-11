---
name: the-first-agent-is-the-progenitor
description: "The agent every version descends from is called the progenitor everywhere; never \"seed agent\" or \"initial agent\""
metadata:
  node_type: memory
  type: feedback
  originSessionId: 9cd172c9-f7d5-4300-a0ea-24a2132f94e2
  modified: 2026-11-11T20:27:18.178Z
---

The agent the tree starts from is the **progenitor** in code, flags, folders, messages, tests, README and issues: `--progenitor_dir`, `--progenitor_name`, `progenitor_swe/`, commit id `progenitor`, `src/rustyhgm/progenitor.rs`. "Root" is kept only for tree-structure code (`Tree::with_root`, `ROOT_COMMIT`, clade roots).

**Why:** the repo had drifted into several names for one thing (seed, initial agent, root), and "seed" also means random-number seeds here.

**How to apply:** don't write "seed agent", "initial agent" or "primogenitor" for it. Mention that the paper calls it the initial agent only where a reader needs the mapping. Random-number seeds keep the word "seed". See [[every-input-is-explicit]].
