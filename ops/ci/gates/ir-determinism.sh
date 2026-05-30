#!/usr/bin/env bash
# GATE: ir-determinism
# Engineering-spec phase: CI compile -> deterministic intermediate representation.
# Validates the deterministic IR-hash invariants and DAG construction tests.
set -uo pipefail

GATE_NAME="ir-determinism"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }
source "${ROOT}/ops/ci/common.sh"

echo "[${GATE_NAME}] cargo test -p jeryu-ci-ir"
if cargo test -p jeryu-ci-ir --jobs "${JERYU_CI_JOBS}"; then
  echo "GATE ${GATE_NAME}: PASS"
  exit 0
else
  echo "GATE ${GATE_NAME}: FAIL (jeryu-ci-ir tests did not pass)"
  exit 1
fi
