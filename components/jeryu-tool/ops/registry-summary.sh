#!/usr/bin/env bash
# Validate and summarize the governed reusable-tool registry with Rust only.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/.." && pwd)"
exec cargo run --quiet --locked --offline \
  --manifest-path "${repo_root}/crates/jeryu-tool-control/Cargo.toml" --bin jeryu-toolctl -- \
  --tool-root "${repo_root}" registry-summary "$@"
