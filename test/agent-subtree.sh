#!/usr/bin/env bash
# Copies only the agent subtree into an empty directory and builds and tests
# it there. This proves that the agent's code does not depend on the
# orchestrator or benchmark crates, which are kept out of its container.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
dest="$(mktemp -d "${TMPDIR:-/tmp}/hgm-agent-subtree.XXXXXX")"
trap 'rm -rf "$dest"' EXIT

paths=(
  Cargo.toml
  rust-toolchain.toml
  src/agent
  lib/agent
  test/unit/agent
  test/integration/agent
)

for p in "${paths[@]}"; do
  mkdir -p "$dest/$(dirname "$p")"
  cp -R "$repo/$p" "$dest/$p"
done

cd "$dest"
cargo build --workspace --all-targets
cargo test --workspace --lib --bins --test unit --test integration
echo "agent subtree OK: $(cargo metadata --no-deps --format-version 1 | grep -o '"name":"hgm[a-z-]*"' | sort -u | tr '\n' ' ')"
