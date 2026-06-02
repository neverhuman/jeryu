#!/usr/bin/env bash
set -euo pipefail
export JERYU_CI_USE_SCCACHE=0
unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_CACHE_SIZE
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_CACHE_SIZE
cd "$(git rev-parse --show-toplevel)/apps/web"
npm ci --include=dev --workspaces=false
npm run typecheck
npm run test
npm run test:e2e
npm run ux-qa
npm run build
