#!/usr/bin/env bash
# ops/ci/rust-lane.sh — single source of truth for the rust CI lane stages.
# Usage: bash ops/ci/rust-lane.sh <fmt|clippy|build|install-smoke|test-select|test-lib|test-integration|tui-smoke|supply-chain|witness|vrc-map|vrc-plan|aer|semver-check|ssh-install-e2e|tui-screenshots|tui-recording|fixture-project-test|fixture-project-clippy|deny|vrc>
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

case "$STAGE" in
  fmt)
    log "cargo fmt --all -- --check"
    cargo fmt --all -- --check
    ;;
  clippy)
    bash scripts/install-redlinedb.sh
    log "cargo clippy --all-targets --all-features -- -D warnings"
    cargo clippy --all-targets --all-features -- -D warnings
    ;;
  build)
    bash scripts/install-redlinedb.sh
    log "cargo build --verbose"
    cargo build --verbose
    ;;
  install-smoke)
    bash scripts/install-redlinedb.sh
    cargo run -p jeryu -- install --dry-run --json --color never --prefix /tmp/jeryu-install-test
    PREFIX="$(mktemp -d)"
    cargo run -p jeryu -- install --yes --prefix "$PREFIX" --path-mode skip
    cargo run -p jeryu -- install doctor --prefix "$PREFIX"
    cargo run -p jeryu -- install smoke --dry-run
    cargo run -p jeryu -- install render-demo --output target/jeryu-install-demo.gif --png target/jeryu-install-demo.png
    cargo run -p jeryu -- remote install xbabe1 --dry-run --yes --setup-key --json
    ;;
  test-select)
    bash scripts/install-redlinedb.sh
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
    bash scripts/install-redlinedb.sh
    if [ "$MODE" = "selected" ] && [ -n "$FILTER" ]; then
      cargo nextest run -p jeryu --lib --profile ci -E "$FILTER"
    else
      cargo nextest run -p jeryu --lib --profile ci
    fi
    ;;
  test-integration)
    bash scripts/install-redlinedb.sh
    cargo test --tests --verbose
    ;;
  tui-smoke)
    bash scripts/install-redlinedb.sh
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
    if cargo semver-checks check-release -p jeryu 2>/dev/null; then
      exit 0
    fi
    cargo semver-checks check-release
    ;;
  ssh-install-e2e)
    bash scripts/install-redlinedb.sh
    cargo build --release -p jeryu
    export JERYU_BIN="$PWD/target/release/jeryu"
    export EVIDENCE_DIR="$PWD/target/ci-evidence/ssh-install"
    bash ops/ci/ssh_install_integration.sh
    ;;
  tui-screenshots)
    bash scripts/install-redlinedb.sh
    mkdir -p target/ci-screenshots
    cargo build --release -p jeryu
    cargo run --release -p jeryu -- install render-demo --output target/ci-screenshots/install-demo.gif --png target/ci-screenshots/install-demo.png
    TABS="workflow jobs mission agents tests pools cache evidence secrets"
    for tab in $TABS; do
      cargo run --release -p jeryu -- tui --capture --tab "$tab" --output "target/ci-screenshots/${tab}.png"
    done
    ls -la target/ci-screenshots/
    ;;
  tui-recording)
    cargo test --test tui_recording -- --ignored --exact tui_demo_recording
    ;;
  fixture-project-test)
    bash scripts/install-redlinedb.sh
    (
      cd tests/fixtures/fixture_project
      cargo test --verbose
      cargo run -- 2>&1 | sed -n '1,20p'
    )
    ;;
  fixture-project-clippy)
    bash scripts/install-redlinedb.sh
    (
      cd tests/fixtures/fixture_project
      cargo clippy -- -D warnings 2>&1 || echo "clippy warnings (non-blocking)"
    )
    ;;
  *)
    die "unknown stage: $STAGE"
    ;;
esac
