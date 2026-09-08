#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=ops/ci/common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
./ops/ci/jankurai.sh
echo "dependency review: cargo audit plus cargo-deny policy"
cargo audit --deny warnings
cargo deny check licenses sources bans advisories
bash ops/ci/dependency-sources.sh
