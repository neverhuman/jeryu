#!/usr/bin/env bash
# GATE: workcells
# Engineering-spec phase: shared workcell control plane proof.
#
# Verifies the narrow workcell surface directly so the per-phase gate summary
# covers the new lifecycle, read-model, and web bootstrap contract without
# relying only on the workspace sweep.
set -uo pipefail

GATE_NAME="workcells"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || {
  echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"
  exit 1
}
source "${ROOT}/ops/ci/common.sh"

echo "[${GATE_NAME}] cargo test -p jeryu-runnerd --jobs ${JERYU_CI_JOBS} workcell"
if ! cargo test -p jeryu-runnerd --jobs "${JERYU_CI_JOBS}" workcell; then
  echo "GATE ${GATE_NAME}: FAIL (jeryu-runnerd workcell tests did not pass)"
  exit 1
fi

echo "[${GATE_NAME}] cargo test -p jeryu-readmodel --jobs ${JERYU_CI_JOBS} workcells"
if ! cargo test -p jeryu-readmodel --jobs "${JERYU_CI_JOBS}" workcells; then
  echo "GATE ${GATE_NAME}: FAIL (jeryu-readmodel workcells tests did not pass)"
  exit 1
fi

echo "[${GATE_NAME}] cargo test -p jeryu-api --features web --jobs ${JERYU_CI_JOBS}"
if ! cargo test -p jeryu-api --features web --jobs "${JERYU_CI_JOBS}"; then
  echo "GATE ${GATE_NAME}: FAIL (jeryu-api web tests did not pass)"
  exit 1
fi

echo "[${GATE_NAME}] cd apps/web && npm ci && npm run typecheck"
if ! (cd apps/web && npm ci --include=dev --workspaces=false 2>/dev/null && npm run typecheck); then
  echo "GATE ${GATE_NAME}: FAIL (web typecheck did not pass)"
  exit 1
fi

echo "GATE ${GATE_NAME}: PASS (runnerd workcell tests; read-model workcells tests; api web tests; web typecheck)"
exit 0
