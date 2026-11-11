#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
dest="$(mktemp -d "${TMPDIR:-/tmp}/rustyhgm-coding-agent-subtree.XXXXXX")"
trap 'rm -rf "$dest"' EXIT

paths=(
  Cargo.toml
  rust-toolchain.toml
  src/coding-agent
  test/src/coding-agent
)

for p in "${paths[@]}"; do
  mkdir -p "$dest/$(dirname "$p")"
  cp -R "$repo/$p" "$dest/$p"
done

cd "$dest"
cargo build --workspace --all-targets
cargo test --workspace --lib --bins --test unit --test integration
echo "agent subtree OK: $(cargo metadata --no-deps --format-version 1 | grep -o '"name":"rustyhgm[a-z-]*"' | sort -u | tr '\n' ' ')"
