#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
./ops/ci/jankurai.sh
echo "dependency review: cargo audit plus cargo-deny policy"
cargo audit --deny warnings
cargo deny check licenses sources bans advisories
