#!/usr/bin/env bash
# Propagate the jankurai pin from tool-manifest.toml into every family consumer.
#
#   ops/render-tool-manifest.sh --check   # family drift lane; never writes
#   ops/render-tool-manifest.sh --repo NAME --repo-root NAME=/absolute/path \
#     --expected-head NAME=40_HEX_SHA
#                                        # exact canonical, custody-checked write
#
# Thin wrapper around the locked, offline Rust control binary.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/.." && pwd)"
exec cargo run --quiet --locked --offline \
  --manifest-path "${repo_root}/crates/jeryu-tool-control/Cargo.toml" --bin jeryu-toolctl -- \
  --tool-root "${repo_root}" render-tool-manifest "$@"
