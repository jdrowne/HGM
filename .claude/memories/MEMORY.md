# Memories

- [Work only on this repo](work-only-on-this-repo.md) — never touch upstream metauto-ai/HGM or other repos; gh writes must target jdrowne/RustyHGM
- [Commit directly to main](commit-directly-to-main.md) — no branches or PRs; push straight to origin main; commits need not be small
- [Always use latest versions](always-use-latest-versions.md) — never pin toolchains, crates, actions or images; stable / "*" / @main / :latest; no committed Cargo.lock
- [No comments in any file](no-comments-in-any-file.md) — no comments, doc comments, docstrings or fixture notes anywhere in the port; reasons go in commits and issues
- [CI follows 10U-Labs conventions](ci-follows-10u-labs-conventions.md) — copy deltahdl and 10U-Labs before inventing a pattern; one workflow per thing guarded; fix code, not limits; stable-only coverage
- [Layout follows 10U-Labs](layout-follows-10u-labs.md) — crates in `src/<component>/` with flat files, no nested src; `lib/<language>/<pkg>` for shared support; `test/src/<component>/<tier>`
- [Supported platforms](supported-platforms.md) — Linux x86_64 and arm64 only, in CI and in releases; no macOS or Windows legs, files or README rows
- [User words are never recorded verbatim](user-words-are-never-recorded-verbatim.md) — restate what the user says in fresh words in memories, issues, commits and summaries; never quote or copy it
- [Long-running commands in the background](long-running-commands-in-the-background.md) — anything past about a minute (tests, coverage, clippy, Docker, CI watch) runs with run_in_background; gh run watch --interval 60
- [Checking locally, then verifying through CI](verifying-through-ci.md) — run locally only the new/touched tests and fmt/clippy/no-comments; workspace-wide tests, coverage gate, Docker tiers and platforms are CI's
- [gh run list --commit takes a full SHA](gh-run-list-commit-takes-a-full-sha.md) — a short SHA returns nothing and a watch loop on it never ends; pass git rev-parse output
- [Memories carry no history](memories-carry-no-history.md) — the rule and its reason only; no dates, requesters, incidents or origins; a changed rule is rewritten, not annotated
- [Explain choices in plain language](explain-choices-in-plain-language.md) — for decisions on unfamiliar tools, show rendered options side by side with a recommendation, not technical questions
- [README diagrams are Mermaid](readme-diagrams-are-mermaid.md) — fenced mermaid GitHub renders; stable kinds only (flowchart with subgraph boxes); nothing newer than GitHub's Mermaid; no %% comments
- [Working without subagents](working-without-subagents.md) — no Agent or Workflow call for any part of the work unless the user's own message asks for one; settings.json denies both
- [Every input is explicit](every-input-is-explicit.md) — every hgm/hgm-agent setting is a required flag; no config file, defaults, environment variables or working-directory lookups
- [Clippy runs with the CI thresholds](clippy-runs-with-ci-thresholds.md) — set CLIPPY_CONF_DIR to etc/clippy/src or test; uvx assert-one-assert-per-test src test; && counts as two asserts
