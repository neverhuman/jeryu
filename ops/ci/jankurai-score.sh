#!/usr/bin/env bash
# Local and CI entrypoint for audit + honest badge check.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
cd "$root"
mkdir -p .jankurai
jankurai audit . --full --mode standard --no-score-history \
  --fail-on critical,high \
  --json .jankurai/repo-score.json \
  --md .jankurai/repo-score.md
jankurai badge --check --update-readme
printf 'jankurai-score ok\n'
