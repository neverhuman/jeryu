#!/usr/bin/env bash
# GATE: runner-sandbox
# Engineering-spec phase: isolated job runners (native + OCI) with a hardened
# sandbox (namespaces, seccomp, no-new-privileges, cgroup limits, and workspace
# file isolation).
#
# Two parts:
#   (A) In-repo suites for the runner crates              -> runnable now.
#   (B) Live namespace / seccomp / cgroup escape suite     -> runnable through
#       the local Docker runtime using the same isolation primitives.
#
# Result policy mirrors git-oracle:
#   - (A) fails      -> GATE FAIL (exit 1).
#   - (B) fails      -> GATE FAIL (exit 1).
#   - both pass      -> GATE PASS (exit 0).
set -uo pipefail

GATE_NAME="runner-sandbox"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }
source "${ROOT}/ops/ci/common.sh"

echo "[${GATE_NAME}] (A) cargo test -p jeryu-runner-core -p jeryu-runner-native -p jeryu-runner-oci -p jeryu-runnerd"
if ! cargo test -p jeryu-runner-core -p jeryu-runner-native -p jeryu-runner-oci -p jeryu-runnerd --jobs "${JERYU_CI_JOBS}"; then
  echo "GATE ${GATE_NAME}: FAIL (runner crate tests did not pass)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: in-repo runner suites passed"

echo "[${GATE_NAME}] (B) live namespace / seccomp / cgroup escape suite"
if ! JERYU_SANDBOX_SKIP_STATIC=1 bash tests/sandbox_escape_matrix.sh; then
  echo "GATE ${GATE_NAME}: FAIL (live sandbox escape matrix failed)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: live sandbox escape matrix passed"

echo "GATE ${GATE_NAME}: PASS (in-repo suites PASS; live sandbox escape matrix PASS)"
exit 0
