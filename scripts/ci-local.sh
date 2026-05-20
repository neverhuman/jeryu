#!/usr/bin/env bash
# scripts/ci-local.sh — run CI lanes locally using the same ops/ci scripts as the workflow
# Usage: scripts/ci-local.sh [fast|audit|quality-gates|doctor|security|proof|tools|bad-behavior|rust <stage>|release-ready|release-preflight <version>]
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

lane="${1:-audit}"
shift || true
case "$lane" in
  fast)
    exec bash "$SCRIPT_DIR/ci-local.sh" quality-gates
    ;;
  audit|security|proof|tools|bad-behavior)
    exec bash "$REPO_ROOT/ops/ci/jankurai-lane.sh" "$lane"
    ;;
  quality-gates)
    exec bash "$REPO_ROOT/ops/ci/quality-gates.sh"
    ;;
  rust)
    if [ "$#" -ne 1 ]; then
      printf 'Usage: scripts/ci-local.sh rust <fmt|clippy|build|deny|witness|vrc|aer>\n' >&2
      exit 2
    fi
    exec bash "$REPO_ROOT/ops/ci/rust-lane.sh" "$1"
    ;;
  release-ready)
    if [ "$#" -ne 0 ]; then
      printf 'Usage: scripts/ci-local.sh release-ready\n' >&2
      exit 2
    fi
    exec bash "$REPO_ROOT/ops/ci/release-ready-lane.sh"
    ;;
  release-preflight)
    if [ "$#" -ne 1 ]; then
      printf 'Usage: scripts/ci-local.sh release-preflight <version>\n' >&2
      exit 2
    fi
    exec bash "$REPO_ROOT/ops/ci/release-lane.sh" preflight "$1"
    ;;
  doctor)
    exec bash "$SCRIPT_DIR/ci-doctor.sh"
    ;;
  *)
    printf 'Unknown lane: %s\nAvailable: fast, audit, security, proof, tools, bad-behavior, quality-gates, rust, release-ready, release-preflight, doctor\n' \
      "$lane" >&2
    exit 1
    ;;
esac
