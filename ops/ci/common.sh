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
  if [ "$crate" = "jeryu-repogate" ]; then
    cargo run -q --release -p "${crate}" -- "$@"
    return
  fi
  cargo run -q --release -p "${crate}" -- "$@"
}

# jeryu_raw_policy <output-path>
#
# Emit a temporary copy of agent/audit-policy.toml without the dead-language
# allowlist so callers can publish the raw report beside the gate.
jeryu_raw_policy() {
  local out="$1"
  awk '
    BEGIN { skip = 0 }
    /^\[dead_language\]$/ { skip = 1; next }
    skip && /^\[/ { skip = 0 }
    !skip { print }
  ' agent/audit-policy.toml > "${out}"
}
