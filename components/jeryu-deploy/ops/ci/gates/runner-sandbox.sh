#!/usr/bin/env bash
# GATE: runner-sandbox
# Engineering-spec phase: isolated job runners (native + OCI) with a hardened
# sandbox (namespaces, seccomp, no-new-privileges, cgroup limits, and workspace
# file isolation).
#
# Two parts:
#   (A) Deploy's owned API integration against the pinned runner release, plus
#       the dependency's runnerd tests that Cargo can execute from this graph.
#   (B) Live namespace / seccomp / cgroup escape suite, runnable through the
#       local Docker runtime using the same isolation primitives.
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

echo "[${GATE_NAME}] (A1) cargo test -p jeryu-runnerd"
if ! cargo test --locked -p jeryu-runnerd --jobs "${JERYU_CI_JOBS}"; then
  echo "GATE ${GATE_NAME}: FAIL (pinned runnerd tests did not pass)"
  exit 1
fi

echo "[${GATE_NAME}] (A2) cargo test -p jeryu-api --features web workcell_run_agent"
if ! cargo test --locked -p jeryu-api --features web --jobs "${JERYU_CI_JOBS}" workcell_run_agent; then
  echo "GATE ${GATE_NAME}: FAIL (Deploy runner integration tests did not pass)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: runner dependency and owned integration tests passed"

echo "[${GATE_NAME}] (B) live namespace / seccomp / cgroup escape suite"
if ! JERYU_SANDBOX_SKIP_STATIC=1 bash tests/sandbox_escape_matrix.sh; then
  echo "GATE ${GATE_NAME}: FAIL (live sandbox escape matrix failed)"
  exit 1
fi
echo "[${GATE_NAME}]   ok: live sandbox escape matrix passed"

echo "GATE ${GATE_NAME}: PASS (runner integration PASS; live sandbox escape matrix PASS)"
exit 0
