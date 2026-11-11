# Memories

- [Work only on this repo](work-only-on-this-repo.md) — never touch upstream metauto-ai/HGM or other repos; gh writes must target jdrowne/RustyHGM
- [Commit directly to main](commit-directly-to-main.md) — no branches or PRs; push straight to origin main; commits need not be small
- [Always use latest versions](always-use-latest-versions.md) — never pin toolchains, crates, actions or images; stable / "*" / @main / :latest; no committed Cargo.lock
- [No comments in any file](no-comments-in-any-file.md) — no comments, doc comments, docstrings or fixture notes anywhere in the port; reasons go in commits and issues
- [CI follows 10U-Labs conventions](ci-follows-10u-labs-conventions.md) — one workflow per thing guarded (release.yml, markdownlint.yml); build per platform; unit, integration and e2e each need only build
- [Layout follows 10U-Labs](layout-follows-10u-labs.md) — crates in `src/<component>/` with flat files, no nested src; `lib/<language>/<pkg>` for shared support; `test/src/<component>/<tier>`
- [Supported platforms](supported-platforms.md) — Linux x86_64/arm64, macOS arm64, Windows x86_64/arm64; integration and e2e only on Linux (Docker)
- [User words are never recorded verbatim](user-words-are-never-recorded-verbatim.md) — restate what the user says in fresh words in memories, issues, commits and summaries; never quote or copy it
- [Long-running commands in the background](long-running-commands-in-the-background.md) — anything past about a minute (tests, coverage, clippy, Docker, CI watch) runs with run_in_background and ends the turn; gh run watch --interval 60
- [Checking locally, then verifying through CI](verifying-through-ci.md) — run locally only the new/touched tests and fmt/clippy/no-comments; workspace-wide tests, coverage gate, Docker tiers and platforms are CI's
- [gh run list --commit takes a full SHA](gh-run-list-commit-takes-a-full-sha.md) — a short SHA returns nothing and a watch loop on it never ends; pass git rev-parse output
