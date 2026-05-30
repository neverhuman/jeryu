#!/usr/bin/env bash
# GATE: proof-gate
# Engineering-spec phase: proof-carrying merges (no-proof-no-merge, owner/
# test-map matching, generated-zone enforcement).
set -uo pipefail

GATE_NAME="proof-gate"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }
source "${ROOT}/ops/ci/common.sh"

echo "[${GATE_NAME}] cargo test -p jeryu-proof"
if cargo test -p jeryu-proof --jobs "${JERYU_CI_JOBS}"; then
  echo "GATE ${GATE_NAME}: PASS"
  exit 0
else
  echo "GATE ${GATE_NAME}: FAIL (jeryu-proof tests did not pass)"
  exit 1
fi
