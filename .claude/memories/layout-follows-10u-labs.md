---
name: layout-follows-10u-labs
description: src/<component>/ holds a crate's files directly (no nested src/); lib/<language>/<package>/ is for shared support code; test/ mirrors the tree
metadata:
  type: feedback
---

# The layout follows the 10U-Labs repos

The tree is laid out like deltahdl and the 10U-Labs repos, not like Cargo's defaults:

- `src/<component>/` holds the component's files directly: `src/core/lib.rs`, `src/rustyhgm/main.rs`. A crate's manifest sets `[lib] path = "lib.rs"` or `[[bin]] path = "main.rs"`, so no crate has a nested `src/`. A component with both a library and a binary, like `src/coding-harness`, is one package.
- `lib/` is grouped by language, `lib/<language>/<package>/` (`lib/python/store`, `lib/cpp/test_helpers`), and holds support code shared across components. HGM's own crates are product code, so they live in `src/`, as deltahdl's lexer and parser do.
- `test/` mirrors the tree: `test/src/<component>/<tier>/main.rs`, with golden files in `test/src/<component>/unit/fixtures/`.

**Why:** deltahdl and the 10U-Labs repos are the reference for layout.

**How to apply:** a new crate goes in `src/<name>/` with flat files and explicit paths in its manifest. Shared Rust test support, if it is ever needed, goes in `lib/rust/<package>/`. Related: [CI follows the 10U-Labs conventions](ci-follows-10u-labs-conventions.md), [No comments in any file](no-comments-in-any-file.md).
