#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
cargo test --workspace --jobs "${JERYU_CI_JOBS}"
cargo build --workspace --release --jobs "${JERYU_CI_JOBS}"
./scripts/emit-release-receipt.sh
