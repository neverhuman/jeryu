#!/usr/bin/env bash
# GATE: git-oracle
# Engineering-spec phase: gitd as a git oracle that is differentially
# bit-for-bit compatible with stock git.
#
# Two parts:
#   (A) In-repo unit/integration suite for jeryu-gitd             -> runnable now.
#   (B) Local differential oracle vs stock bare Git repositories  -> runnable now.
#
# Result policy:
#   - If (A) or (B) fails -> GATE FAIL  (exit 1).
#   - If both pass        -> GATE PASS  (exit 0).
set -uo pipefail

GATE_NAME="git-oracle"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }
source "${ROOT}/ops/ci/common.sh"

echo "[${GATE_NAME}] (A) cargo test -p jeryu-gitd  (in-repo suite)"
if ! cargo test -p jeryu-gitd --jobs "${JERYU_CI_JOBS}"; then
  echo "GATE ${GATE_NAME}: FAIL (jeryu-gitd in-repo tests did not pass)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: in-repo jeryu-gitd suite passed"

echo "[${GATE_NAME}] (B) local differential-vs-stock-git suite"
if ! cargo test -p jeryu-gitd --test oracle_differential --jobs "${JERYU_CI_JOBS}"; then
  echo "GATE ${GATE_NAME}: FAIL (local differential-vs-stock-git suite did not pass)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: local differential oracle passed"

echo "GATE ${GATE_NAME}: PASS (jeryu-gitd suite + local differential oracle passed)"
exit 0
