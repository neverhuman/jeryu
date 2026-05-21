#!/usr/bin/env bash
# ops/ci/rust-lane.sh — single source of truth for the rust CI lane stages.
# Usage: bash ops/ci/rust-lane.sh <fmt|clippy|build|install-smoke|test-select|test-lib|test-integration|tui-smoke|supply-chain|witness|vrc-map|vrc-plan|aer|semver-check|hardening|ssh-install-e2e|tui-screenshots|tui-recording|fixture-project-test|fixture-project-clippy|deny|vrc>
#
# Local rehearsals and `.github/workflows/rust.yml` both call into this so
# CI/local parity stays true (jankurai HLT-042).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$SCRIPT_DIR/lib.sh"
cd "$REPO_ROOT"
require_tool cargo

STAGE="${1:-}"
if [ -z "$STAGE" ]; then
  die "usage: bash ops/ci/rust-lane.sh <stage>"
fi

write_github_output() {
  local key="$1"
  local value="$2"
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    printf '%s=%s\n' "$key" "$value" >> "$GITHUB_OUTPUT"
  fi
}

install_redlinedb_if_requested() {
  local backend="${JERYU_DB_BACKEND:-sqlite}"
  local url="${JERYU_DATABASE_URL:-}"
  case "${backend,,}" in
    redline|redlinedb)
      bash scripts/install-redlinedb.sh
      return
      ;;
  esac
  case "${url,,}" in
    redline:*|redlinedb:*)
      bash scripts/install-redlinedb.sh
      return
      ;;
  esac
  log "skip RedlineDB binary install for SQLite backend"
}

case "$STAGE" in
  fmt)
    log "cargo fmt --all -- --check"
    cargo fmt --all -- --check
    ;;
  clippy)
    install_redlinedb_if_requested
    log "cargo clippy --workspace --all-targets --all-features -- -D warnings"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    ;;
  build)
    install_redlinedb_if_requested
    log "cargo build --workspace --verbose"
    cargo build --workspace --verbose
    ;;
  install-smoke)
    install_redlinedb_if_requested
    cargo run -p jeryu -- install --dry-run --json --color never --prefix /tmp/jeryu-install-test
    PREFIX="$(mktemp -d)"
    cargo run -p jeryu -- install --yes --prefix "$PREFIX" --path-mode skip
    cargo run -p jeryu -- install doctor --prefix "$PREFIX"
    cargo run -p jeryu -- install smoke --dry-run
    cargo run -p jeryu -- install render-demo --output target/jeryu-install-demo.gif --png target/jeryu-install-demo.png
    cargo run -p jeryu -- remote install xbabe1 --dry-run --yes --setup-key --json
    ;;
  test-select)
    install_redlinedb_if_requested
    mkdir -p target/jeryu
    cargo build --bin jeryu
    if ./target/debug/jeryu test select \
      --base origin/main \
      --head HEAD \
      --explain \
      --emit-plan target/jeryu/test-plan.json 2>/dev/null; then
      MODE="$(jq -r '.mode' target/jeryu/test-plan.json)"
      UNIT_FILTER_EXPR=""
      while IFS= read -r expr; do
        [ -n "$expr" ] || continue
        if [ -z "$UNIT_FILTER_EXPR" ]; then
          UNIT_FILTER_EXPR="$expr"
        else
          UNIT_FILTER_EXPR="$UNIT_FILTER_EXPR | $expr"
        fi
      done < <(
        jq -r '
          .selected_tests[]
          | select(.kind == "unit_filter")
          | .command
        ' target/jeryu/test-plan.json | sed -e "s/^cargo nextest run -E '//" -e "s/'$//"
      )
    else
      echo '{"mode":"full","selected_tests":[]}' > target/jeryu/test-plan.json
      MODE="full"
      UNIT_FILTER_EXPR=""
    fi
    write_github_output mode "$MODE"
    write_github_output unit_filter_expr "$UNIT_FILTER_EXPR"
    echo "VTI mode: $MODE"
    echo "VTI unit filter: ${UNIT_FILTER_EXPR:-(full run)}"
    ;;
  test-lib)
    MODE="${2:-full}"
    FILTER="${3:-}"
    install_redlinedb_if_requested
    if [ "$MODE" = "selected" ] && [ -n "$FILTER" ]; then
      cargo nextest run -p jeryu --lib --profile ci -E "$FILTER"
    else
      cargo nextest run -p jeryu --lib --profile ci
    fi
    ;;
  test-integration)
    install_redlinedb_if_requested
    cargo test --tests --verbose -- --test-threads=1
    ;;
  tui-smoke)
    install_redlinedb_if_requested
    cargo run -- tui --once
    ;;
  supply-chain)
    bash tools/security-lane.sh .
    ;;
  deny)
    log "cargo deny check"
    cargo deny check
    ;;
  witness)
    log "cargo run -p cargo-witness -- build"
    cargo run -p cargo-witness -- build
    ;;
  vrc|vrc-map)
    log "cargo run -p cargo-vrc -- map --output-dir ."
    cargo run -p cargo-vrc -- map --output-dir .
    ;;
  vrc-plan)
    git diff --name-only origin/main...HEAD > changed-files.txt
    cat changed-files.txt
    mapfile -t CHANGED < changed-files.txt
    if [ "${#CHANGED[@]}" -gt 0 ]; then
      cargo run -p cargo-vrc -- plan "${CHANGED[@]}" --output vrc-plan.json
    else
      echo '{"mode":"full","reason":"no changed files detected"}' > vrc-plan.json
    fi
    ;;
  aer)
    log "cargo run -p cargo-aer -- scan --output aer-findings.json"
    cargo run -p cargo-aer -- scan --output aer-findings.json
    ;;
  semver-check)
    BASELINE_REV="${SEMVER_BASELINE_REV:-origin/main}"
    if git rev-parse --verify "${BASELINE_REV}^{commit}" >/dev/null 2>&1; then
      cargo semver-checks check-release -p jeryu --baseline-rev "$BASELINE_REV"
    else
      if cargo semver-checks check-release -p jeryu 2>/dev/null; then
        exit 0
      fi
      cargo semver-checks check-release
    fi
    ;;
  hardening)
    mkdir -p target/hardening
    log "cargo semver-checks check-release"
    SEMVER_STATUS=0
    bash "$0" semver-check || SEMVER_STATUS=$?
    log "cargo run -p cargo-aer -- scan --output target/hardening/aer-findings.json"
    cargo run -p cargo-aer -- scan --output target/hardening/aer-findings.json
    if [ "$SEMVER_STATUS" -ne 0 ]; then
      exit "$SEMVER_STATUS"
    fi
    ;;
  ssh-install-e2e)
    install_redlinedb_if_requested
    cargo build --release -p jeryu
    export JERYU_BIN="$PWD/target/release/jeryu"
    export EVIDENCE_DIR="$PWD/target/ci-evidence/ssh-install"
    bash ops/ci/ssh_install_integration.sh
    ;;
  tui-screenshots)
    install_redlinedb_if_requested
    mkdir -p target/ci-screenshots
    cargo build --release -p jeryu
    cargo run --release -p jeryu -- install render-demo --output target/ci-screenshots/install-demo.gif --png target/ci-screenshots/install-demo.png
    TABS="workflow jobs mission agents tests bugs pools cache evidence secrets"
    for tab in $TABS; do
      cargo run --release -p jeryu -- tui --capture --tab "$tab" --output "target/ci-screenshots/${tab}.png"
    done
    ls -la target/ci-screenshots/
    ;;
  tui-recording)
    cargo test --test tui_recording -- --ignored --exact tui_demo_recording
    ;;
  fixture-project-test)
    install_redlinedb_if_requested
    (
      cd tests/fixtures/fixture_project
      cargo test --verbose
      cargo run -- 2>&1 | sed -n '1,20p'
    )
    ;;
  fixture-project-clippy)
    install_redlinedb_if_requested
    (
      cd tests/fixtures/fixture_project
      cargo clippy -- -D warnings 2>&1 || echo "clippy warnings (non-blocking)"
    )
    ;;
  *)
    die "unknown stage: $STAGE"
    ;;
esac
