---
name: readme-diagrams-are-mermaid
description: "Markdown diagrams are Mermaid blocks GitHub renders itself; stable diagram kinds only (flowchart, groups as boxes), nothing newer than GitHub's Mermaid"
metadata:
  node_type: memory
  type: project
  originSessionId: 41e6a54b-2690-4a77-89f7-18b94e3f6652
  modified: 2026-11-11T13:30:14.185Z
---

# README diagrams are Mermaid

Diagrams in the repo's Markdown are fenced `mermaid` blocks that GitHub renders natively, never images built in CI. Use only stable diagram kinds. The concept view is a `flowchart` whose groups are subgraph boxes, not the beta swimlane kind. Use no feature newer than the Mermaid version GitHub runs; a fenced block holding just `info` previewed on GitHub prints that version. Mermaid's `%%` comments and `%%{init}%%` are out under [[no-comments-in-any-file]]; YAML front matter `config:` is allowed. Labels and shapes carry meaning instead of hard-coded fills, because GitHub switches between light and dark.

**Why:** The README must render on GitHub with no build step, and a beta kind can change syntax and break the README when GitHub upgrades Mermaid.

**How to apply:** Render drafts locally with mermaid-cli at GitHub's Mermaid version before pushing, and show the user. A CI workflow draws every Markdown diagram with the newest Mermaid as an early warning, per [[always-use-latest-versions]] and [[ci-follows-10u-labs-conventions]].
