#!/usr/bin/env bash
# Compatibility entrypoint: embedded monorepo source is verified, never replaced.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
case $#:${1:-} in
  0:|1:--plan) ;;
  1:--help|1:-h)
    printf 'usage: scripts/fetch-family.sh [--plan]\nVerifies embedded source; component repositories are downstream exports.\n'
    exit 0
    ;;
  *)
    printf 'Component reconstruction is unavailable. Build the tracked monorepo source; family.lock.toml records historical exports.\n' >&2
    exit 2
    ;;
esac
exec cargo run --locked --manifest-path "$root/Cargo.toml" \
  -p jeryu-split-tool --bin jeryu-split -- monorepo-check
