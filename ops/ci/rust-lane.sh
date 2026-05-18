#!/usr/bin/env bash
# ops/ci/rust-lane.sh — single source of truth for the rust CI lane stages.
# Usage: bash ops/ci/rust-lane.sh <fmt|clippy|build|deny|witness|vrc|aer>
#
# Local rehearsals and `.github/workflows/rust.yml` both call into this so
# CI/local parity stays true (jankurai HLT-042).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$SCRIPT_DIR/lib.sh"
cd "$REPO_ROOT"

STAGE="${1:-}"
if [ -z "$STAGE" ]; then
  die "usage: bash ops/ci/rust-lane.sh <fmt|clippy|build|deny|witness|vrc|aer>"
fi

case "$STAGE" in
  fmt)
    log "cargo fmt --all -- --check"
    cargo fmt --all -- --check
    ;;
  clippy)
    log "cargo clippy --all-targets --all-features -- -D warnings"
    cargo clippy --all-targets --all-features -- -D warnings
    ;;
  build)
    log "cargo build --release -p jeryu"
    cargo build --release -p jeryu
    ;;
  deny)
    log "cargo deny check"
    cargo deny check
    ;;
  witness)
    log "cargo run -p cargo-witness -- build"
    cargo run -p cargo-witness -- build
    ;;
  vrc)
    log "cargo run -p cargo-vrc -- map --output-dir ."
    cargo run -p cargo-vrc -- map --output-dir .
    ;;
  aer)
    log "cargo run -p cargo-aer -- scan --output aer-findings.json"
    cargo run -p cargo-aer -- scan --output aer-findings.json
    ;;
  *)
    die "unknown stage: $STAGE"
    ;;
esac
