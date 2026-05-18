#!/usr/bin/env bash
# scripts/pre-pr.sh — local pre-PR checks for the Evidence Gate work.
#
# Runs in order:
#   1. cargo check -p jeryu --tests        (fast)
#   2. cargo test  -p jeryu --lib          (~470 unit tests)
#   3. cargo test  --test autonomy_e2e     (mock e2e)
#   4. scripts/local-live.sh all           (live LLM + provider sweep)
#
# Prints a one-line summary at the end. Exit code mirrors the worst result.

set -e
set -o pipefail

cd "$(dirname "$0")/.."

if [[ "${CI:-}" == "true" || "${CI:-}" == "1" ]]; then
  echo "error: pre-pr.sh is for local pre-PR use; CI has its own lane" >&2
  exit 2
fi

STAGES=()
RESULTS=()

run_stage() {
  local name="$1"; shift
  echo
  echo "════════════════════════════════════════"
  echo "==> $name"
  echo "════════════════════════════════════════"
  STAGES+=("$name")
  if "$@"; then
    RESULTS+=("PASS")
  else
    RESULTS+=("FAIL")
  fi
}

run_stage "cargo fmt --check" cargo fmt --all -- --check
run_stage "cargo check --tests" cargo check -p jeryu --tests --message-format=short
run_stage "cargo test --lib autonomy::" cargo test -p jeryu --lib autonomy::
run_stage "cargo test --lib llm::"      cargo test -p jeryu --lib llm::
run_stage "cargo test --lib agent_review::" cargo test -p jeryu --lib agent_review::
run_stage "cargo test --lib approval::" cargo test -p jeryu --lib approval::
run_stage "cargo test --lib git_host::" cargo test -p jeryu --lib git_host::
run_stage "cargo test --test autonomy_e2e (mock end-to-end)" \
  cargo test --test autonomy_e2e
run_stage "cargo test --test cli_smoke (CLI surface)" \
  cargo test --test cli_smoke
run_stage "cargo test --test coverage_more (edge cases)" \
  cargo test --test coverage_more
run_stage "cargo deny check (supply chain)" cargo deny check
run_stage "scripts/local-live.sh (live LLM + doctor + GitHub + full-spine)" \
  ./scripts/local-live.sh all

echo
echo "════════════════════════════════════════"
echo "  pre-pr summary"
echo "════════════════════════════════════════"
RC=0
for i in "${!STAGES[@]}"; do
  R="${RESULTS[$i]}"
  S="${STAGES[$i]}"
  printf "  %-6s  %s\n" "$R" "$S"
  if [[ "$R" != "PASS" ]]; then
    RC=1
  fi
done
exit "$RC"
