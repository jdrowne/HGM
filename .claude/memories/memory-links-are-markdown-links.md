---
name: memory-links-are-markdown-links
description: "Links between memory files are standard Markdown links to the file, [Heading](slug.md), never [[slug]] wiki links, which GitHub shows as raw brackets"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 0643b937-64dd-4a3e-81d3-8ee86b22c55a
  modified: 2026-11-12T02:20:52.609Z
---

# Memory links are Markdown links

A memory that points at another memory uses a standard Markdown link to its file: `[Verifying through CI](verifying-through-ci.md)`, with the target's H1 heading as the text. Never write `[[slug]]`, even though Claude Code's memory instructions ask for that form.

**Why:** the memory files are committed to the repository and read as Markdown. Markdown has no `[[...]]` syntax, so GitHub renders those links as literal brackets that go nowhere, while a file link works both on GitHub and for recall.

**How to apply:** use this form whenever a memory is written or edited, and convert any `[[slug]]` found in an existing memory. Link only to a memory file that exists. TOML section headers such as `[[bin]]` inside code spans are not links and stay as they are. Each file still starts with its heading: [Memory files start with a heading](memory-files-start-with-a-heading.md).
