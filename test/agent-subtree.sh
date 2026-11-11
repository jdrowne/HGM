#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
dest="$(mktemp -d "${TMPDIR:-/tmp}/hgm-agent-subtree.XXXXXX")"
trap 'rm -rf "$dest"' EXIT

paths=(
  Cargo.toml
  rust-toolchain.toml
  src/agent
  test/src/agent
)

for p in "${paths[@]}"; do
  mkdir -p "$dest/$(dirname "$p")"
  cp -R "$repo/$p" "$dest/$p"
done

cd "$dest"
cargo build --workspace --all-targets
cargo test --workspace --lib --bins --test unit --test integration
echo "agent subtree OK: $(cargo metadata --no-deps --format-version 1 | grep -o '"name":"hgm[a-z-]*"' | sort -u | tr '\n' ' ')"
