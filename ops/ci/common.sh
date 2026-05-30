#!/usr/bin/env bash
# Shared local CI defaults. Keep this file source-only.
set -euo pipefail

export JERYU_CI_JOBS="${JERYU_CI_JOBS:-40}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-${JERYU_CI_JOBS}}"

# jeryu_gate <crate> [args...]
#
# Invoke one of the Rust governance/CI gate binaries. Prefers the prebuilt
# release binary under target/release/<crate> for speed, and transparently
# falls back to `cargo run -q --release -p <crate> --` when that binary is
# absent (e.g. a clean checkout that has not run the build step yet).
jeryu_gate() {
  local crate="$1"; shift
  local bin="target/release/${crate}"
  if [ -x "${bin}" ]; then
    "${bin}" "$@"
  else
    cargo run -q --release -p "${crate}" -- "$@"
  fi
}
