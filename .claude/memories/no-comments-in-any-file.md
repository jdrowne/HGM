---
name: no-comments-in-any-file
description: No tracked file in the port carries a comment or docstring, in any language, even where assert-no-comments cannot read it
metadata:
  type: feedback
---

# No comments in any file

No file of the port carries a comment, doc comment or docstring: Rust (`//`, `///`, `//!`, `/* */`), TOML, YAML, shell, Python, `.gitignore` and `.dockerignore` alike. Golden fixtures carry no `notes` field either. What a comment would say goes in the commit message or the issue.

**Why:** the user asked for assert-no-comments to be adopted as it is in deltahdl and the 10U-Labs repos, whose rule is that nothing tracked carries a comment, including files the tool cannot read. The tool reads Rust, but not TOML or the ignore files, so for `Cargo.toml`, `rust-toolchain.toml`, `.gitignore` and `.dockerignore` the rule is kept by hand.

**How to apply:** strip comments before committing; the `assert-no-comments` job in `rustyhgm.yml` lists the trees it guards. The upstream Python files are left as they are, since they are deleted at cutover. Related: [[ci-follows-10u-labs-conventions]].
