#!/usr/bin/env bash
# ops/ci/quality-gates.sh — lightweight quality gate for pre-push and local validation
# Usage: bash ops/ci/quality-gates.sh
# CI:    called from ops/git-hooks/pre-push
# Local: scripts/ci-local.sh quality-gates
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$SCRIPT_DIR/lib.sh"
cd "$REPO_ROOT"
ensure_dirs
require_jankurai
require_tool cargo

log "jankurai doctor"
jankurai doctor --fail-on high

log "cargo check"
mkdir -p target/jankurai/cache
CARGO_INCREMENTAL=0 cargo check --workspace

log "nextest lib"
CARGO_INCREMENTAL=0 cargo nextest run -p jeryu --lib

ok "quality-gates passed"
