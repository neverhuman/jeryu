#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
echo "security lane: cache adversary, poisoning matrix, zero-evidence, secret scanning, and dependency scanning"
cargo test -p jeryu-cache-adversary --jobs "${JERYU_CI_JOBS}"
./tests/cache_poisoning_matrix.sh
jeryu_gate jeryu-evidence .
./scripts/secret-scan.sh
cargo audit --deny warnings
cargo deny check licenses sources bans advisories
