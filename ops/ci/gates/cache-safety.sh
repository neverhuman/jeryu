#!/usr/bin/env bash
# GATE: cache-safety
# Engineering-spec phase: content-addressed build cache with poisoning-resistant
# safety laws.
#
# Parts:
#   (A) In-repo cache suites (jeryu-cache-core, jeryu-cache, and the
#       jeryu-cache-adversary crate when present)          -> runnable now.
#   (B) Live cache-poisoning harness                        -> needs a running
#       cache service + adversarial network harness, not runnable here.
#       Reported as PENDING; NEVER reported as PASS.
#
# Result policy:
#   - (A) fails  -> GATE FAIL (exit 1).
#   - (A) passes -> GATE PENDING (exit 0).
set -uo pipefail

GATE_NAME="cache-safety"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }

# Base cache packages.
PKGS="-p jeryu-cache-core -p jeryu-cache"

# Optional adversary crate: include only if it exists in the workspace.
if [ -d crates/jeryu-cache-adversary ]; then
  PKGS="${PKGS} -p jeryu-cache-adversary"
  echo "[${GATE_NAME}] adversary crate present: including jeryu-cache-adversary"
else
  echo "[${GATE_NAME}] adversary crate absent: skipping jeryu-cache-adversary"
fi

echo "[${GATE_NAME}] (A) cargo test ${PKGS}"
# shellcheck disable=SC2086
if ! cargo test ${PKGS}; then
  echo "GATE ${GATE_NAME}: FAIL (cache crate tests did not pass)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: in-repo cache suites passed"

echo "[${GATE_NAME}] (B) live cache-poisoning harness"
echo "[${GATE_NAME}]   PENDING: live poisoning harness (needs running cache service)"

echo "GATE ${GATE_NAME}: PENDING (in-repo suites PASS; live poisoning harness not yet wired)"
exit 0
