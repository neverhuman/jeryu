#!/usr/bin/env bash
set -euo pipefail
export JERYU_CI_USE_SCCACHE=0
unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_CACHE_SIZE
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_CACHE_SIZE
cd "$(git rev-parse --show-toplevel)/apps/web"
rm -rf dist storybook-static playwright-report test-results
npm ci --include=dev --workspaces=false
npx playwright install chromium
npm run typecheck
npm run test
npm run test:contracts
npm run build
npm run test:e2e
npm run build-storybook
# ux-qa runs the @jankurai/ux-qa rendered-UX lane (ux-qa/ux-qa-check.mjs) over the
# built SPA + Storybook: Playwright screenshot / visual review baselines, axe
# accessibility receipts, and the analyzePage geometry runtime (getBoundingClientRect
# edge clearance + WCAG 2.5.5 target size). Receipts land in target/jankurai/ux-qa/.
npm run ux-qa
