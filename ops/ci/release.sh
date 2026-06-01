#!/usr/bin/env bash
set -euo pipefail
export JERYU_CI_USE_SCCACHE=0
unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_CACHE_SIZE
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_CACHE_SIZE
cargo test --workspace --jobs "${JERYU_CI_JOBS}"
cargo build --workspace --release --jobs "${JERYU_CI_JOBS}"
./scripts/emit-release-receipt.sh
