#!/usr/bin/env bash
# GATE: cache-safety
# Engineering-spec phase: Deploy's cache client contract.
#
# The poisoning-resistant cache implementation and adversarial matrix live in
# the standalone jeryu-cache repository. Deploy owns only the `jeryu cache
# self-test` client/CLI boundary, so this gate tests exactly that boundary and
# does not mislabel foreign source as an in-repo suite.
set -uo pipefail

GATE_NAME="cache-safety"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }
source "${ROOT}/ops/ci/common.sh"

echo "[${GATE_NAME}] cargo test -p jeryu-cli --test cli_snapshots dispatch_release_and_cache_self_test"
if ! cargo test --locked -p jeryu-cli --test cli_snapshots \
  --jobs "${JERYU_CI_JOBS}" dispatch_release_and_cache_self_test; then
  echo "GATE ${GATE_NAME}: FAIL (Deploy cache client/CLI contract did not pass)"
  exit 1
fi

echo "GATE ${GATE_NAME}: PASS (Deploy cache client/CLI contract; product cache safety remains owned by jeryu-cache)"
exit 0
