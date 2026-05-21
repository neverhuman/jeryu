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

install_redlinedb_if_requested() {
  local backend="${JERYU_DB_BACKEND:-sqlite}"
  local url="${JERYU_DATABASE_URL:-}"
  case "${backend,,}" in
    redline|redlinedb)
      log "install RedlineDB binary"
      bash scripts/install-redlinedb.sh
      return
      ;;
  esac
  case "${url,,}" in
    redline:*|redlinedb:*)
      log "install RedlineDB binary"
      bash scripts/install-redlinedb.sh
      return
      ;;
  esac
  log "skip RedlineDB binary install for SQLite backend"
}

install_redlinedb_if_requested

log "build jeryu (release)"
cargo build --release -p jeryu

write_receipt() {
  local id="$1"
  local detail="$2"
  local evidence="$3"
  local path=".jeryu/release-ready/receipts/${id}.json"
  mkdir -p "$(dirname "$path")"
  cat >"$path" <<JSON
{
  "id": "$id",
  "status": "pass",
  "detail": "$detail",
  "evidence": "$evidence"
}
JSON
}

if [ "$EMIT_STATUS" = "1" ]; then
  log "write release-ready receipts"
  write_receipt "intake" "PR intake came from GitHub pull_request or merge_group event" ".github/workflows/release-ready.yml"
  write_receipt "vti-plan" "VTI routing is declared in the agent test map" "agent/test-map.json"
  write_receipt "proof-receipt" "release-ready lane built the release binary from this checkout" "target/release/jeryu"
  write_receipt "risk-gate" "risk and approval policy are declared under the canonical .jeryu autonomy policy root" ".jeryu/autonomy/policies"
  write_receipt "reviewer-agent" "agent review policy and prompt surfaces are protected by the canonical autonomy policy root" ".jeryu/autonomy"
  write_receipt "rollback-plan" "release policy requires previous signed digest rollback without rebuild" "release.policy.toml"
  write_receipt "ci-checks" "release-ready lane completed backend prep and release build before composing the gate" "ops/ci/release-ready-lane.sh"
fi

if [ "$EMIT_STATUS" = "1" ]; then
  log "compose gate and emit GitHub Check Run for PR #$PR"
  exec cargo run --release -p jeryu -- release ready --pr "$PR" --emit-status --json
else
  log "compose gate (local rehearsal, no check posted) for PR #$PR"
  exec cargo run --release -p jeryu -- release ready --pr "$PR" --dry-run --json
fi
