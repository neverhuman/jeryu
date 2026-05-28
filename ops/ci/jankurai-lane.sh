#!/usr/bin/env bash
# ops/ci/jankurai-lane.sh — run jankurai audit, proof, and tool lanes
# Usage: bash ops/ci/jankurai-lane.sh <security|audit|proof|tools|bad-behavior|sbom|all>
#
# Env vars (CI-only, all optional):
#   JANKURAI_SARIF_OUT     — SARIF output path for the audit step
#   JANKURAI_SUMMARY_OUT   — GitHub step-summary path for the audit step
#   JANKURAI_REPAIR_QUEUE  — repair-queue JSONL path for the audit step
#   JANKURAI_BASELINE      — override baseline path (default: agent/repo-score.json)
#   JANKURAI_AUDIT_MODE    — audit mode (default: advisory)
#   JANKURAI_CHANGED_FROM  — changed-path base ref for proofbind (default: origin/main)
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
  log "README jankurai score freshness"
  bash scripts/sync-readme-jankurai-score.sh \
    --check \
    --score-json target/jankurai/repo-score.json \
    --readme README.md
}

is_proofbind_binary_artifact() {
  local path="$1"
  case "$path" in
    assets/*.gif|assets/*.png) return 0 ;;
    *) return 1 ;;
  esac
}

is_utf8_file() {
  local path="$1"
  [ ! -s "$path" ] && return 0
  iconv -f UTF-8 -t UTF-8 "$path" >/dev/null 2>&1
}

ensure_proofbind_base_ref() {
  local base="$1"
  local depth="${JANKURAI_BASE_FETCH_DEPTH:-50}"

  if git rev-parse --verify "$base^{commit}" >/dev/null 2>&1; then
    return 0
  fi

  case "$base" in
    origin/*)
      local branch="${base#origin/}"
      if git remote get-url origin >/dev/null 2>&1; then
        log "fetch proofbind base ref: $base"
        git fetch --no-tags --depth="$depth" origin \
          "+refs/heads/${branch}:refs/remotes/origin/${branch}" >/dev/null 2>&1 || true
      fi
      ;;
    refs/remotes/origin/*)
      local branch="${base#refs/remotes/origin/}"
      if git remote get-url origin >/dev/null 2>&1; then
        log "fetch proofbind base ref: $base"
        git fetch --no-tags --depth="$depth" origin \
          "+refs/heads/${branch}:${base}" >/dev/null 2>&1 || true
      fi
      ;;
  esac

  git rev-parse --verify "$base^{commit}" >/dev/null 2>&1
}

write_empty_proofbind_outputs() {
  log "proofbind changed set has no text inputs after binary filtering"
  local head generated
  head="$(git rev-parse --short HEAD 2>/dev/null || printf 'unknown')"
  generated="$(date +%s)"
  mkdir -p target/jankurai/proofbind
  cat >target/jankurai/proofbind/surface-witness.json <<JSON
{"schema_version":"1.0.0","standard_version":"0.7.0","generated_at":"$generated","repo_root":".","git_head":"$head","mode":"advisory","changed_paths":[],"surfaces":[],"summary":{"changed_surface_count":0,"high_or_critical_surface_count":0,"by_surface_type":{},"by_owner":{},"verdict":"pass"}}
JSON
  cat >target/jankurai/proofbind/obligations.json <<JSON
{"schema_version":"1.0.0","standard_version":"0.7.0","generated_at":"$generated","repo_root":".","git_head":"$head","mode":"advisory","obligations":[],"summary":{"total":0,"satisfied":0,"missing":0,"high_or_critical_missing":0,"changed_surface_count":0,"verdict":"pass"}}
JSON
  cat >target/jankurai/proofbind/proofbind.md <<'MD'
# Proofbind

No UTF-8 changed files remained after binary artifact filtering.
MD
}

proofbind_changed_args() {
  local base="${JANKURAI_CHANGED_FROM:-origin/main}"
  local path
  local skipped=0
  local diff_ref="$base...HEAD"
  PROOFBIND_CHANGED_ARGS=()

  if ! ensure_proofbind_base_ref "$base"; then
    die "proofbind base ref is not available: $base"
  fi

  while IFS= read -r path; do
    [ -n "$path" ] || continue
    [ -f "$path" ] || continue
    if is_proofbind_binary_artifact "$path"; then
      log "proofbind skip generated binary artifact: $path"
      skipped=$((skipped + 1))
      continue
    fi
    if ! is_utf8_file "$path"; then
      die "proofbind cannot read non-UTF-8 changed file: $path"
    fi
    PROOFBIND_CHANGED_ARGS+=(--changed "$path")
  done < <(git diff --name-only "$diff_ref")

  if [ "$skipped" -gt 0 ]; then
    log "proofbind skipped $skipped generated binary artifact(s)"
  fi
}

run_proof() {
  log "proofbind verify"
  proofbind_changed_args
  if [ "${#PROOFBIND_CHANGED_ARGS[@]}" -eq 0 ]; then
    write_empty_proofbind_outputs
  else
    jankurai proofbind verify . "${PROOFBIND_CHANGED_ARGS[@]}"
  fi
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
  local smoke_port="$1"
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
url = "http://127.0.0.1:$smoke_port/index.html"
states = ["loading", "empty", "error", "success", "permission-denied"]
EOF
  printf '%s\n' "$smoke_dir/ux-qa.toml"
}

start_ux_qa_smoke_server() {
  local smoke_dir="$1"
  local smoke_port_file="$2"
  local smoke_log="$3"
  local smoke_port="$4"
  python3 -m http.server "$smoke_port" --bind 127.0.0.1 --directory "$smoke_dir" >"$smoke_log" 2>&1 &
  local smoke_pid=$!
  printf '%s\n' "$smoke_pid" >"$smoke_port_file"
  for _ in 1 2 3 4 5; do
    if python3 - "$smoke_port" <<'PY' >/dev/null 2>&1
import socket
import sys

port = int(sys.argv[1])
with socket.create_connection(("127.0.0.1", port), timeout=1):
    pass
PY
    then
      return 0
    fi
    sleep 1
  done
  return 1
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
  local smoke_port smoke_pid smoke_log smoke_dir
  smoke_dir="target/jankurai/ux-qa-smoke"
  smoke_port="$(python3 - <<'PY'
import socket
with socket.socket() as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
  smoke_log="target/jankurai/ux-qa-smoke/http.log"
  ux_cli="$(prepare_ux_qa_cli)"
  ux_config="$(prepare_ux_qa_smoke_config "$smoke_port")"
  start_ux_qa_smoke_server "$smoke_dir" "target/jankurai/ux-qa-smoke/http.pid" "$smoke_log" "$smoke_port"
  smoke_pid="$(cat target/jankurai/ux-qa-smoke/http.pid)"
  trap 'if [ -n "${smoke_pid:-}" ] && kill "$smoke_pid" >/dev/null 2>&1; then wait "$smoke_pid" >/dev/null 2>&1 || true; fi' EXIT
  node "$ux_cli" audit --config "$ux_config" \
    --out target/jankurai/ux-qa.json
  if kill "$smoke_pid" >/dev/null 2>&1; then
    wait "$smoke_pid" >/dev/null 2>&1 || true
  fi
  trap - EXIT
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

run_sbom() {
  log "Generate CycloneDX SBOM"
  mkdir -p target/jankurai/sbom
  cargo cyclonedx --format json --override-filename sbom
  local f rel safe
  while IFS= read -r f; do
    rel="${f#./}"
    safe="${rel//\//__}"
    mv "$f" "target/jankurai/sbom/$safe"
  done < <(
    find . -maxdepth 4 -type f \
      \( -name 'sbom.cdx.json' -o -name 'sbom.json' \) \
      -not -path './target/jankurai/*'
  )
}

# ── Dispatch ───────────────────────────────────────────────────────────────

cmd="${1:-all}"
case "$cmd" in
  security)     run_security ;;
  audit)        run_audit ;;
  proof)        run_proof ;;
  tools)        run_tools ;;
  bad-behavior) run_bad_behavior ;;
  sbom)         run_sbom ;;
  all)
    run_security
    run_audit
    run_proof
    run_tools
    run_bad_behavior
    run_sbom
    ;;
  *)
    die "Unknown command: $cmd. Valid: security, audit, proof, tools, bad-behavior, sbom, all"
    ;;
esac

ok "$cmd lane complete"
