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
require_jankurai

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
    --full \
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

prepare_ux_qa_cli() {
  require_tool git
  require_tool node
  require_tool npm
  require_tool npx

  local tool_root="${JANKURAI_UX_QA_TOOL_DIR:-target/jankurai/ux-qa-tool}"
  local checkout="$tool_root/jankurai"
  local cli="$checkout/packages/ux-qa/dist/cli.js"
  if [ ! -f "$cli" ]; then
    rm -rf "$tool_root"
    mkdir -p "$tool_root"
    git clone --branch "v$JANKURAI_REQUIRED_VERSION" --depth 1 https://github.com/neverhuman/jankurai "$checkout" >/dev/null 2>&1
    (
      cd "$checkout"
      npm ci >/dev/null 2>&1
      npm run ux-qa:build >/dev/null 2>&1
    )
  fi
  (
    cd "$checkout"
    npx playwright install chromium --with-deps >/dev/null 2>&1 \
      || npx playwright install chromium >/dev/null 2>&1
  )
  printf '%s\n' "$cli"
}

prepare_ux_qa_smoke_config() {
  local smoke_dir="target/jankurai/ux-qa-smoke"
  mkdir -p "$smoke_dir"
  cat >"$smoke_dir/index.html" <<'HTML'
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Jankurai UX QA Smoke</title>
  <style>
    * { box-sizing: border-box; }
    body { margin: 0; min-height: 100vh; display: grid; place-items: center; font-family: system-ui, sans-serif; background: #f6f7fb; color: #151923; }
    main { width: min(560px, calc(100vw - 48px)); padding: 32px; border: 1px solid #d9deea; background: #fff; }
    h1 { margin: 0 0 12px; font-size: 2rem; line-height: 1.1; }
    p { margin: 0 0 18px; color: #455166; line-height: 1.5; }
    button { border: 0; padding: 12px 18px; font: inherit; font-weight: 700; background: #153d6f; color: #fff; }
  </style>
</head>
<body>
  <main>
    <h1 id="state-label">Loading</h1>
    <p id="state-copy">The smoke page exercises required rendered states.</p>
    <button type="button">Primary action</button>
  </main>
  <script>
    const state = new URLSearchParams(location.search).get("state") ?? "loading";
    const copy = {
      loading: ["Loading", "The page is waiting on data."],
      empty: ["Empty", "The page has no records to show."],
      error: ["Error", "The page encountered a recoverable failure."],
      success: ["Success", "The page loaded successfully."],
      "permission-denied": ["Permission denied", "The user lacks access to this view."]
    }[state] ?? [state, "Unknown state."];
    document.getElementById("state-label").textContent = copy[0];
    document.getElementById("state-copy").textContent = copy[1];
  </script>
</body>
</html>
HTML
  cat >"$smoke_dir/ux-qa.toml" <<EOF
artifactRoot = "target/jankurai/ux-qa"
readyState = "domcontentloaded"
timeoutMs = 15000
screenshotRequired = true
ariaSnapshotRequired = true
accessibilityScanRequired = true
requiredStates = ["loading", "empty", "error", "success", "permission-denied"]
stateQueryParam = "state"

[[routes]]
id = "ux-qa-smoke"
url = "file://$REPO_ROOT/target/jankurai/ux-qa-smoke/index.html"
states = ["loading", "empty", "error", "success", "permission-denied"]
EOF
  printf '%s\n' "$smoke_dir/ux-qa.toml"
}

run_tools() {
  log "workspace map (VRC)"
  cargo run -p cargo-vrc -- map --output-dir .
  log "AER structural scan"
  cargo run -p cargo-aer -- scan --output target/jankurai/aer-findings.json
  log "migration analyze"
  jankurai migrate . --analyze --out target/jankurai/migration-report.json
  log "UX QA smoke"
  local ux_cli ux_config
  ux_cli="$(prepare_ux_qa_cli)"
  ux_config="$(prepare_ux_qa_smoke_config)"
  node "$ux_cli" audit --config "$ux_config" \
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
