#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

mkdir -p .jankurai
jankurai . \
  --json .jankurai/repo-score.json \
  --md .jankurai/repo-score.md \
  --fail-under "${JERYU_JANKURAI_FAIL_UNDER:-85}"
