#!/usr/bin/env bash
# ops/ci/jankurai-lane.sh — run jankurai audit, proof, and tool lanes
# Usage: bash ops/ci/jankurai-lane.sh <security|audit|ratchet|proof|tools|bad-behavior|sbom|all>
#
# Env vars (CI-only, all optional):
#   JANKURAI_SARIF_OUT     — SARIF output path for the audit step
#   JANKURAI_SUMMARY_OUT   — GitHub step-summary path for the audit step
#   JANKURAI_REPAIR_QUEUE  — repair-queue JSONL path for the audit step
#   JANKURAI_BASELINE      — override baseline path (default: agent/repo-score.json)
#   JANKURAI_AUDIT_MODE    — audit mode (default: advisory)
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$SCRIPT_DIR/lib.sh"
cd "$REPO_ROOT"
ensure_dirs
require_tool jankurai

# ── Sub-commands ───────────────────────────────────────────────────────────

run_security() {
  log "security strict preflight"
  jankurai security run . --strict --profile ci \
    --out target/jankurai/security/evidence.json
}

run_audit() {
  log "jankurai advisory audit"
  local baseline="${JANKURAI_BASELINE:-agent/repo-score.json}"
  local mode="${JANKURAI_AUDIT_MODE:-advisory}"
  local extra=()
  [ -n "${JANKURAI_SARIF_OUT:-}"    ] && extra+=(--sarif              "$JANKURAI_SARIF_OUT")
  [ -n "${JANKURAI_SUMMARY_OUT:-}"  ] && extra+=(--github-step-summary "$JANKURAI_SUMMARY_OUT")
  [ -n "${JANKURAI_REPAIR_QUEUE:-}" ] && extra+=(--repair-queue-jsonl  "$JANKURAI_REPAIR_QUEUE")
  jankurai audit . \
    --mode "$mode" \
    --baseline "$baseline" \
    --json target/jankurai/repo-score.json \
    --md  target/jankurai/repo-score.md \
    "${extra[@]}"
}

run_proof() {
  log "proofbind verify"
  jankurai proofbind verify . --changed-from origin/main
  log "proofmark rust"
  jankurai proofmark rust . \
    --obligations target/jankurai/proofbind/obligations.json
  log "rust witness build"
  jankurai rust witness build .
}

run_tools() {
  log "workspace map (VRC)"
  cargo run -p cargo-vrc -- map --output-dir .
  log "AER structural scan"
  cargo run -p cargo-aer -- scan --output target/jankurai/aer-findings.json
  log "migration analyze"
  jankurai migrate . --analyze --json target/jankurai/migration-report.json
  log "UX QA smoke"
  jankurai ux audit --config agent/ux-qa.toml \
    --out target/jankurai/ux-qa.json
}

run_bad_behavior() {
  log "language bad-behavior"
  cargo test -p jeryu --test language_bad_behavior \
    -- --test-threads=1
  log "ci-bad-behavior"
  cargo test -p jeryu --test language_bad_behavior \
    ci_bad_behavior_lane_is_blocking -- --exact --test-threads=1
  log "git-bad-behavior"
  cargo test -p jeryu --test language_bad_behavior \
    git_bad_behavior_lane_is_blocking -- --exact --test-threads=1
  log "release-bad-behavior"
  cargo test -p jeryu --test language_bad_behavior \
    release_bad_behavior_lane_is_blocking -- --exact --test-threads=1
}

# ── Dispatch ───────────────────────────────────────────────────────────────

cmd="${1:-all}"
case "$cmd" in
  security)     run_security ;;
  audit)        run_audit ;;
  proof)        run_proof ;;
  tools)        run_tools ;;
  bad-behavior) run_bad_behavior ;;
  all)
    run_security
    run_audit
    run_proof
    run_tools
    run_bad_behavior
    ;;
  *)
    die "Unknown command: $cmd. Valid: security, audit, proof, tools, bad-behavior, all"
    ;;
esac

ok "$cmd lane complete"
