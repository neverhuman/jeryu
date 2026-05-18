#!/usr/bin/env bash
# scripts/ci-local.sh — run CI lanes locally using the same ops/ci scripts as the workflow
# Usage: scripts/ci-local.sh [audit|quality-gates|doctor|security|proof|tools|bad-behavior]
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

lane="${1:-audit}"
case "$lane" in
  audit|security|proof|tools|bad-behavior)
    exec bash "$REPO_ROOT/ops/ci/jankurai-lane.sh" "$lane"
    ;;
  quality-gates)
    exec bash "$REPO_ROOT/ops/ci/quality-gates.sh"
    ;;
  doctor)
    exec bash "$SCRIPT_DIR/ci-doctor.sh"
    ;;
  *)
    printf 'Unknown lane: %s\nAvailable: audit, security, proof, tools, bad-behavior, quality-gates, doctor\n' \
      "$lane" >&2
    exit 1
    ;;
esac
