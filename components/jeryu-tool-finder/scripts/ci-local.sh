#!/usr/bin/env bash
set -euo pipefail

usage='usage: ci-local.sh {required|security|score|contract-drift|artifact-support}'

if [[ "$#" -ne 1 ]]; then
  printf '%s\n' "$usage" >&2
  exit 2
fi

case "$1" in
  required)
    lane_script='ops/ci/pr-ci.sh'
    ;;
  security)
    lane_script='tools/security-lane.sh'
    ;;
  score)
    lane_script='ops/ci/score.sh'
    ;;
  contract-drift)
    lane_script='ops/ci/contract-drift.sh'
    ;;
  artifact-support)
    lane_script='ops/ci/artifact-support.sh'
    ;;
  *)
    printf 'unsupported CI lane: %s\n' "$1" >&2
    exit 2
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root"

exec bash "$repo_root/$lane_script"
