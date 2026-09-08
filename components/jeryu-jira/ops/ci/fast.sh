#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
cargo fmt --all --check
cargo check --workspace --all-targets --jobs "${JERYU_CI_JOBS:-40}"
printf 'fast ok: %s\n' "$(pwd)"
