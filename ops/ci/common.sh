#!/usr/bin/env bash
# Shared local CI defaults. Keep this file source-only.
set -euo pipefail

export JERYU_CI_JOBS="${JERYU_CI_JOBS:-40}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-${JERYU_CI_JOBS}}"

# jeryu_gate <crate> [args...]
#
# Invoke one of the Rust governance/CI gate binaries through Cargo's release
# runner so local gates never execute stale binaries from a previous build.
jeryu_gate() {
  local crate="$1"; shift
  cargo run -q --release -p "${crate}" -- "$@"
}
