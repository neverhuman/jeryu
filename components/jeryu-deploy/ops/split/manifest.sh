#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
export JERYU_SPLIT_MANIFEST_PROGRAM="$0"
exec cargo run --locked --offline --quiet --manifest-path "$repo_root/Cargo.toml" \
  -p jeryu-split-tool --bin jeryu-split -- manifest "$@"
