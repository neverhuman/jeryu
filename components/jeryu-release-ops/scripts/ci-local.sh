#!/usr/bin/env bash
set -euo pipefail

lane="${1:-required}"
case "$lane" in
  required)
    just release-readiness
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
