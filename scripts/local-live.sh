#!/usr/bin/env bash
# scripts/local-live.sh — run the Evidence Gate live test suite against local keys.
#
# Pre-PR usage:
#   ./scripts/local-live.sh             # all live tests
#   ./scripts/local-live.sh smoke       # OpenRouter smoke only
#   ./scripts/local-live.sh doctor      # provider sweep only
#   ./scripts/local-live.sh e2e         # full-spine end-to-end live
#
# Secrets are read via the canonical chain (env → ~/.jeryu/secrets/llm.env →
# repo .env.local). CI mode (CI=true) refuses local files for safety.
#
# This script sets JERYU_LLM_LIVE=1 so the `#[ignore]`-gated tests run.
# It MUST NOT be invoked from CI; CI lanes never source it.

set -e
set -o pipefail

cd "$(dirname "$0")/.."

if [[ "${CI:-}" == "true" || "${CI:-}" == "1" ]]; then
  echo "error: local-live.sh refuses to run in CI; live tests are pre-PR only" >&2
  exit 2
fi

# Resolve at least OPENROUTER_API_KEY from the standard chain before running.
HAVE_KEY=0
if [[ -n "${OPENROUTER_API_KEY:-}" ]]; then
  HAVE_KEY=1
elif [[ -f "$HOME/.jeryu/secrets/llm.env" ]] && grep -q '^OPENROUTER_API_KEY=' "$HOME/.jeryu/secrets/llm.env"; then
  HAVE_KEY=1
elif [[ -f .env.local ]] && grep -q '^OPENROUTER_API_KEY=' .env.local; then
  HAVE_KEY=1
fi

if [[ "$HAVE_KEY" != "1" ]]; then
  echo "error: no OPENROUTER_API_KEY found in the canonical secrets chain" >&2
  echo "  add it to one of:" >&2
  echo "    - env var OPENROUTER_API_KEY=..." >&2
  echo "    - ~/.jeryu/secrets/llm.env (canonical user default)" >&2
  echo "    - ./.env.local (repo-local, gitignored)" >&2
  exit 3
fi

# Build everything once so the test runs that follow don't spend on compile.
echo "==> compiling tests (one-time)..."
cargo test --no-run -p jeryu --tests --quiet

SUBSET="${1:-all}"
export JERYU_LLM_LIVE=1
RC=0

run_one() {
  local test_file="$1"
  local name="$2"
  echo "==> running $test_file::$name"
  if ! cargo test --test "$test_file" -- --ignored --nocapture "$name"; then
    echo "FAIL: $test_file::$name" >&2
    RC=1
  fi
}

case "$SUBSET" in
  smoke)
    run_one llm_smoke_openrouter live_security_review_flags_sql_injection
    run_one llm_smoke_openrouter live_security_review_passes_clean_diff
    run_one llm_smoke_openrouter live_secret_scrub_aborts_before_calling_llm
    ;;
  doctor)
    run_one llm_doctor sweep_all_providers
    ;;
  e2e)
    run_one autonomy_e2e_live full_spine_live_sqli_lands_reject
    ;;
  github)
    run_one git_host_github_live ping_user_returns_login
    run_one git_host_github_live approve_mr_dry_run_path_works_live
    ;;
  all|*)
    run_one llm_doctor sweep_all_providers
    run_one git_host_github_live ping_user_returns_login
    run_one git_host_github_live approve_mr_dry_run_path_works_live
    run_one llm_smoke_openrouter live_security_review_flags_sql_injection
    run_one llm_smoke_openrouter live_security_review_passes_clean_diff
    run_one llm_smoke_openrouter live_secret_scrub_aborts_before_calling_llm
    run_one autonomy_e2e_live full_spine_live_sqli_lands_reject
    ;;
esac

if [[ "$RC" == "0" ]]; then
  echo
  echo "✓ all live tests passed"
else
  echo
  echo "✗ at least one live test failed" >&2
fi
exit "$RC"
