#!/usr/bin/env bash
# ops/ci/release-ready-lane.sh — compose the jeryu/release-ready composite gate
# Usage: bash ops/ci/release-ready-lane.sh [<pr_number>]
#
# This is the SINGLE source of truth for what `jeryu/release-ready` runs.
# CI (`.github/workflows/release-ready.yml`) and local rehearsals both call it.
#
# Env (optional, all read by `jeryu release ready`):
#   GITHUB_REPOSITORY  — owner/repo, required when --emit-status is set by caller
#   GITHUB_SHA         — head sha for the Check Run, same constraint
#   GITHUB_TOKEN       — gh auth, same constraint
#   JERYU_EMIT_STATUS  — when set to "1", post the composite check to GitHub
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$SCRIPT_DIR/lib.sh"
cd "$REPO_ROOT"

PR="${1:-${PR_NUMBER:-0}}"
EMIT_STATUS="${JERYU_EMIT_STATUS:-0}"

log "install RedlineDB binary"
bash scripts/install-redlinedb.sh

log "build jeryu (release)"
cargo build --release -p jeryu

if [ "$EMIT_STATUS" = "1" ]; then
  log "compose gate and emit GitHub Check Run for PR #$PR"
  exec cargo run --release -p jeryu -- release ready --pr "$PR" --emit-status --json
else
  log "compose gate (local rehearsal, no check posted) for PR #$PR"
  exec cargo run --release -p jeryu -- release ready --pr "$PR" --dry-run --json
fi
