#!/usr/bin/env bash
# ops/ci/web-lane.sh — single source of truth for the web CI lane stages.
# Usage: bash ops/ci/web-lane.sh <rust-fmt-clippy-check|rust-nextest-lib|rust-nextest-integration|drift-types|drift-schemas|frontend|e2e|lighthouse>
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$SCRIPT_DIR/lib.sh"
cd "$REPO_ROOT"

STAGE="${1:-}"

usage() {
  die "usage: bash ops/ci/web-lane.sh <stage>"
}

install_rust_prereqs() {
  bash "$SCRIPT_DIR/install-mold.sh"
}

run_frontend_bundle_budget() {
  local main
  main="$(find apps/web/dist/assets -name 'index-*.js' -print -quit)"
  if [ -z "$main" ]; then
    die "main bundle not found under apps/web/dist/assets/"
  fi
  local size budget
  size="$(gzip -c "$main" | wc -c)"
  budget=358400
  log "main bundle: ${size} bytes gz (budget ${budget})"
  test "$size" -le "$budget"
}

run_bff() {
  local log_file="${BFF_LOG:-/tmp/bff.log}"
  : >"$log_file"
  JERYU_WEB_TRUST_LOCAL="${JERYU_WEB_TRUST_LOCAL:-1}" \
  JERYU_LOCAL_USERS="${JERYU_LOCAL_USERS:-alice:repo.read,repo.write|bob:repo.read}" \
  JERYU_PLAYWRIGHT_E2E_MODE="${JERYU_PLAYWRIGHT_E2E_MODE:-bff-only}" \
    ./target/release/jeryu web serve \
      --bind 127.0.0.1:8787 \
      --spa-dir apps/web/dist >"$log_file" 2>&1 &
  BFF_PID=$!
  export BFF_PID

  cleanup() {
    if [ -n "${BFF_PID:-}" ] && kill -0 "$BFF_PID" 2>/dev/null; then
      kill "$BFF_PID" 2>/dev/null || true
    fi
  }
  trap cleanup EXIT

  for i in $(seq 1 30); do
    if curl -fs http://127.0.0.1:8787/health >/dev/null 2>&1; then
      log "BFF up after ${i}s"
      break
    fi
    sleep 1
  done

  curl -fs http://127.0.0.1:8787/health
  (cd apps/web && npx playwright test --reporter=html,line)
}

case "$STAGE" in
  rust-fmt-clippy-check)
    install_rust_prereqs
    cargo fmt --all -- --check
    cargo check --workspace --features web
    cargo clippy --workspace --features web --all-targets -- -D warnings
    ;;
  rust-nextest-lib)
    install_rust_prereqs
    cargo nextest run -p jeryu --features web --lib --no-fail-fast
    ;;
  rust-nextest-integration)
    install_rust_prereqs
    cargo nextest run -p jeryu --features web --tests --no-fail-fast
    ;;
  drift-types)
    cargo run --bin jeryu_export_types --features web
    git diff --exit-code contracts/generated/
    ;;
  drift-schemas)
    cargo run --bin jeryu_export_schemas --features web
    git diff --exit-code schemas/
    ;;
  frontend)
    npm ci --no-audit --no-fund
    npm --workspace @jeryu/web run typecheck
    npm --workspace @jeryu/web run lint
    npm --workspace @jeryu/web run test
    npm --workspace @jeryu/web run build
    npm --workspace @jeryu/web run build-storybook
    run_frontend_bundle_budget
    ;;
  e2e)
    npm ci --no-audit --no-fund
    npm --workspace @jeryu/web run build
    install_rust_prereqs
    cargo build --release --features web -p jeryu
    (cd apps/web && npx playwright install chromium --with-deps)
    run_bff
    ;;
  lighthouse)
    npm ci --no-audit --no-fund
    npm --workspace @jeryu/web run build
    npm --workspace @jeryu/web run perf
    ;;
  *)
    usage
    ;;
esac
