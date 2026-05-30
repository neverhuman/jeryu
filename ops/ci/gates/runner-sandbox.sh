#!/usr/bin/env bash
# GATE: runner-sandbox
# Engineering-spec phase: isolated job runners (native + OCI) with a hardened
# sandbox (seccomp / Landlock / cgroups).
#
# Two parts:
#   (A) In-repo suites for the runner crates              -> runnable now.
#   (B) Live seccomp / Landlock / cgroups escape suite     -> needs the native
#       sandbox runtime (privileged kernel features), not available here.
#       Reported as PENDING; NEVER reported as PASS.
#
# Result policy mirrors git-oracle:
#   - (A) fails  -> GATE FAIL (exit 1).
#   - (A) passes -> GATE PENDING (exit 0).
set -uo pipefail

GATE_NAME="runner-sandbox"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }

echo "[${GATE_NAME}] (A) cargo test -p jeryu-runner-core -p jeryu-runner-native -p jeryu-runner-oci -p jeryu-runnerd"
if ! cargo test -p jeryu-runner-core -p jeryu-runner-native -p jeryu-runner-oci -p jeryu-runnerd; then
  echo "GATE ${GATE_NAME}: FAIL (runner crate tests did not pass)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: in-repo runner suites passed"

echo "[${GATE_NAME}] (B) live seccomp / Landlock / cgroups escape suite"
echo "[${GATE_NAME}]   PENDING: live sandbox-escape suite (needs native sandbox runtime)"

echo "GATE ${GATE_NAME}: PENDING (in-repo suites PASS; live sandbox runtime not yet wired)"
exit 0
