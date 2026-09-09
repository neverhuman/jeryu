#!/usr/bin/env bash
set -euo pipefail

[[ $# -le 1 ]] || { printf 'expected at most one CI lane\n' >&2; exit 2; }
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root"
lane="${1-required}"
case "$lane" in
  required)
    exec bash ops/ci/pr-ci.sh
    ;;
  fast)
    just fast
    ;;
  check)
    just check
    ;;
  score)
    just score
    ;;
  security)
    just security
    ;;
  artifact-support)
    just artifact-support
    ;;
  contract-drift)
    just redline-consumer-test
    ;;
  *)
    printf 'unsupported CI lane: %s\n' "$lane" >&2
    exit 2
    ;;
esac
