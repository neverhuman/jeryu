#!/usr/bin/env bash
# Shared local CI defaults. Keep this file source-only.
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ci-env.sh"

# jeryu_gate <crate> [args...]
#
# Invoke one of the Rust governance/CI gate binaries through Cargo's release
# runner so local gates never execute stale binaries from a previous build.
jeryu_gate() {
  local crate="$1"; shift
  cargo run -q --release -p "${crate}" -- "$@"
}
