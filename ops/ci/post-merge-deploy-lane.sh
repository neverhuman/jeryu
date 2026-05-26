#!/usr/bin/env bash
# ops/ci/post-merge-deploy-lane.sh — single source of truth for the
# .github/workflows/post-merge-deploy.yml stages.
#
# Stages:
#   gate <event-name> <conclusion> <branch>   — emit "proceed" or "skip" + reason
#   build <ref>                                — rebuild and verify release artifact at the given SHA
#   verify                                     — post-install verification via scripts/deploy-local.sh
#
# Local rehearsal:
#   bash ops/ci/post-merge-deploy-lane.sh gate workflow_run success main
#   bash ops/ci/post-merge-deploy-lane.sh build "$(git rev-parse HEAD)"
#   bash ops/ci/post-merge-deploy-lane.sh verify
#
# This script is the canonical CI parity surface for the post-merge deploy
# pipeline (jankurai HLT-042 — workflows must delegate to ops/ci/*.sh
# rather than inline cargo/install commands).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

usage() {
    printf 'usage: %s {gate <event> <conclusion> <branch> | build <ref> | verify | install-from-artifact <path>}\n' "$0" >&2
    exit 2
}

stage_gate() {
    local event="${1:-}" conclusion="${2:-}" branch="${3:-}"
    if [ "$event" = "workflow_dispatch" ]; then
        echo "proceed	manual"
        return 0
    fi
    if [ "$event" != "workflow_run" ]; then
        echo "skip	non-workflow_run event"
        return 0
    fi
    if [ "$conclusion" != "success" ]; then
        echo "skip	triggering Rust run ended with conclusion='$conclusion'"
        return 0
    fi
    if [ "$branch" != "main" ]; then
        echo "skip	triggering Rust run on branch='$branch' (not main)"
        return 0
    fi
    echo "proceed	rust-green-on-main"
}

stage_build() {
    local ref="${1:-}"
    [ -n "$ref" ] || { echo "build: ref required" >&2; exit 2; }
    echo "==> Building release binary at ref=$ref"
    git -C "$REPO_ROOT" checkout "$ref"
    bash "$REPO_ROOT/scripts/deploy-local.sh" --check
}

stage_verify() {
    bash "$REPO_ROOT/scripts/deploy-local.sh" --verify
}

stage_install_from_artifact() {
    local artifact_path="${1:-target/release/jeryu}"
    [ -x "$artifact_path" ] || { echo "install-from-artifact: $artifact_path is not executable" >&2; exit 2; }
    local install_dir="${JERYU_INSTALL_PREFIX:-$HOME/.local}/bin"
    mkdir -p "$install_dir"
    install -m 0755 "$artifact_path" "$install_dir/jeryu"
    bash "$REPO_ROOT/scripts/deploy-local.sh" --verify
}

case "${1:-}" in
    gate) shift; stage_gate "$@" ;;
    build) shift; stage_build "$@" ;;
    verify) stage_verify ;;
    install-from-artifact) shift; stage_install_from_artifact "$@" ;;
    *) usage ;;
esac
