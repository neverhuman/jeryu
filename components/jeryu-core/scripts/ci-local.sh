#!/usr/bin/env bash
set -euo pipefail

if (( $# == 0 )); then
  just fast
  just check
elif (( $# == 1 )) && [[ $1 == required ]]; then
  repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
  cd "$repo_root"
  exec bash ops/ci/pr-ci.sh
else
  printf 'usage: scripts/ci-local.sh [required]\n' >&2
  exit 2
fi
