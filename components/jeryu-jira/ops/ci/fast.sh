#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
# shellcheck source=ops/ci/cargo-scope.sh
source ops/ci/cargo-scope.sh
cargo fmt --manifest-path "$member_manifest" --package "${owned_packages[@]}" --check
cargo check --locked --manifest-path "$member_manifest" --package "${owned_packages[@]}" --all-targets --jobs "${JERYU_CI_JOBS:-40}"
printf 'fast ok: %s\n' "$(pwd)"
