#!/usr/bin/env bash
# GATE: foundation
# Engineering-spec phase: cross-cutting baseline (fmt / check / clippy / test /
# zero-evidence guard / docs / release receipt / repo score).
# Delegates to the canonical ops/ci/full.sh so this gate stays in lock-step
# with the project's existing definition of "green".
set -uo pipefail

GATE_NAME="foundation"
# Resolve repo root from this script's location (ops/ci/gates/foundation.sh).
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }

if [ ! -x ops/ci/full.sh ] && [ ! -f ops/ci/full.sh ]; then
  echo "GATE ${GATE_NAME}: FAIL (ops/ci/full.sh not found)"
  exit 1
fi

echo "[${GATE_NAME}] delegating to ops/ci/full.sh ..."
if bash ops/ci/full.sh; then
  echo "GATE ${GATE_NAME}: PASS"
  exit 0
else
  rc=$?
  echo "GATE ${GATE_NAME}: FAIL (ops/ci/full.sh exit ${rc})"
  exit 1
fi
