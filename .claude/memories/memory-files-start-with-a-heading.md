---
name: memory-files-start-with-a-heading
description: "Every memory file puts an H1 title right after its front matter, because the markdownlint workflow lints .claude/memories with MD041"
metadata:
  node_type: memory
  type: feedback
  originSessionId: cd429840-fa72-4fe1-98f5-b941d3d8b7fa
  modified: 2026-11-11T21:20:01.959Z
---

# Memory files start with a heading

Every file in `.claude/memories/` has a `# <Title>` line straight after the closing `---` of its front matter, with the same title as its `MEMORY.md` entry, and the body follows it.

**Why:** the markdownlint workflow runs with `--dot`, so it lints the memories, and MD041 fails any file whose first content line is not a top-level heading.

**How to apply:** write the heading whenever a memory is created or rewritten. To lint locally, pass only tracked files (`git ls-files '*.md'`), because `target/` and `venv/` hold untracked Markdown that CI never sees. See [CI follows the 10U-Labs conventions](ci-follows-10u-labs-conventions.md).
