#!/usr/bin/env bash
# Shared local CI defaults. Keep this file source-only.
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ci-env.sh"

JERYU_CI_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
JERYU_JANKURAI_VERSION="${JANKURAI_VERSION:-jankurai 1.6.10}"
JERYU_JANKURAI_BIN="${JERYU_JANKURAI_BIN:-${CARGO_HOME:-$HOME/.cargo}/bin/jankurai}"

# ensure_pinned_jankurai
#
# Local hosts may have ~/.local/bin before ~/.cargo/bin on PATH. Always verify
# and execute the pinned Cargo-installed auditor so stale PATH shadows cannot
# change audit semantics in the middle of a lane.
ensure_pinned_jankurai() {
  bash "${JERYU_CI_ROOT}/ops/ci/ensure-jankurai.sh" >/dev/null
  if [ ! -x "${JERYU_JANKURAI_BIN}" ]; then
    echo "pinned jankurai binary missing: ${JERYU_JANKURAI_BIN}" >&2
    return 1
  fi
  if ! "${JERYU_JANKURAI_BIN}" --version | grep -qx "${JERYU_JANKURAI_VERSION}"; then
    echo "wrong pinned jankurai version at ${JERYU_JANKURAI_BIN}: $("${JERYU_JANKURAI_BIN}" --version 2>&1 || true)" >&2
    return 1
  fi
}

jeryu_jankurai() {
  ensure_pinned_jankurai
  "${JERYU_JANKURAI_BIN}" "$@"
}

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
