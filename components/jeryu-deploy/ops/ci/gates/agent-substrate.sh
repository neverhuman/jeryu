#!/usr/bin/env bash
# GATE: agent-substrate
# Engineering-spec phase: in-cell agent execution substrate.
#
# Deploy does not own the agentbridge or egress crates; those are immutable
# dependencies released by their standalone repositories. Verify the owned API
# integration points that exercise the pinned agentbridge instead of trying to
# run foreign packages as if they were workspace members. Live LLM/network
# calls remain explicitly budget- and secret-gated and are not launched here.
set -uo pipefail

GATE_NAME="agent-substrate"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || {
  echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"
  exit 1
}
source "${ROOT}/ops/ci/common.sh"

echo "[${GATE_NAME}] cargo test -p jeryu-api --features web workcell_run_agent"
if ! cargo test --locked -p jeryu-api --features web --jobs "${JERYU_CI_JOBS}" workcell_run_agent; then
  echo "GATE ${GATE_NAME}: FAIL (workcell run-agent integration tests did not pass)"
  exit 1
fi

echo "[${GATE_NAME}] cargo test -p jeryu-api --features web agent_runs"
if ! cargo test --locked -p jeryu-api --features web --jobs "${JERYU_CI_JOBS}" agent_runs; then
  echo "GATE ${GATE_NAME}: FAIL (agent-run control integration tests did not pass)"
  exit 1
fi

echo "GATE ${GATE_NAME}: PASS (owned workcell and agent-run integrations against pinned agentbridge)"
exit 0
