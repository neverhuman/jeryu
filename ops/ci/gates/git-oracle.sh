#!/usr/bin/env bash
# GATE: git-oracle
# Engineering-spec phase: gitd as a git oracle that is differentially
# bit-for-bit compatible with stock git.
#
# Two parts:
#   (A) In-repo unit/integration suite for jeryu-gitd  -> runnable now.
#   (B) Live differential-vs-stock-git suite           -> needs a RUNNING gitd
#       daemon, which is not wired up in this environment yet. It is reported
#       as PENDING and is NEVER reported as PASS.
#
# Result policy:
#   - If (A) fails              -> GATE FAIL  (exit 1).
#   - If (A) passes             -> GATE PENDING (exit 0): runnable part green,
#                                  live oracle still to be built/wired.
set -uo pipefail

GATE_NAME="git-oracle"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }

echo "[${GATE_NAME}] (A) cargo test -p jeryu-gitd  (in-repo suite)"
if ! cargo test -p jeryu-gitd; then
  echo "GATE ${GATE_NAME}: FAIL (jeryu-gitd in-repo tests did not pass)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: in-repo jeryu-gitd suite passed"

# (B) live differential oracle: not runnable here.
echo "[${GATE_NAME}] (B) live differential-vs-stock-git suite"
echo "[${GATE_NAME}]   PENDING: live git-oracle (needs gitd daemon)"

echo "GATE ${GATE_NAME}: PENDING (in-repo suite PASS; live oracle not yet wired)"
exit 0
