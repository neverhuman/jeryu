#!/usr/bin/env bash
# GATE: cache-safety
# Engineering-spec phase: content-addressed build cache with poisoning-resistant
# safety laws.
#
# Parts:
#   (A) In-repo cache suites (jeryu-cache-core, jeryu-cache-service,
#       jeryu-cache, and the jeryu-cache-adversary crate when present)
#       -> runnable now.
#   (B) Local cache-poisoning/false-hit harness             -> runnable now.
#
# Result policy:
#   - (A) fails  -> GATE FAIL (exit 1).
#   - (B) fails  -> GATE FAIL (exit 1).
#   - both pass  -> GATE PASS (exit 0).
set -uo pipefail

GATE_NAME="cache-safety"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }
source "${ROOT}/ops/ci/common.sh"

# Base cache packages.
PKGS="-p jeryu-cache-core -p jeryu-cache-service -p jeryu-cache"

# Optional adversary crate: include only if it exists in the workspace.
if [ -d crates/jeryu-cache-adversary ]; then
  PKGS="${PKGS} -p jeryu-cache-adversary"
  echo "[${GATE_NAME}] adversary crate present: including jeryu-cache-adversary"
else
  echo "[${GATE_NAME}] adversary crate absent: skipping jeryu-cache-adversary"
fi

echo "[${GATE_NAME}] (A) cargo test ${PKGS}"
# shellcheck disable=SC2086
if ! cargo test ${PKGS} --jobs "${JERYU_CI_JOBS}"; then
  echo "GATE ${GATE_NAME}: FAIL (cache crate tests did not pass)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: in-repo cache suites passed"

echo "[${GATE_NAME}] (B) local cache-poisoning/false-hit harness"
if ! ./tests/cache_poisoning_matrix.sh; then
  echo "GATE ${GATE_NAME}: FAIL (local cache-poisoning/false-hit harness did not pass)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: local poisoning/false-hit harness passed"

echo "GATE ${GATE_NAME}: PASS (in-repo suites PASS; local poisoning/false-hit harness PASS)"
exit 0
