#!/usr/bin/env bash
# GATE: agent-substrate
# Engineering-spec phase: in-cell agent execution substrate.
#
# Verifies the deterministic edit-bot driver, adversarial parallel staging
# tests, and the opt-in live egress runtime contract. Live LLM/network calls are
# not launched by this gate; they remain explicitly budget- and secret-gated.
set -uo pipefail

GATE_NAME="agent-substrate"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || {
  echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"
  exit 1
}
source "${ROOT}/ops/ci/common.sh"

echo "[${GATE_NAME}] cargo test -p jeryu-agentbridge -p jeryu-egress --jobs ${JERYU_CI_JOBS}"
if ! cargo test -p jeryu-agentbridge -p jeryu-egress --jobs "${JERYU_CI_JOBS}"; then
  echo "GATE ${GATE_NAME}: FAIL (agentbridge/egress tests did not pass)"
  exit 1
fi

echo "GATE ${GATE_NAME}: PASS (agentbridge driver/adversarial tests; egress contract tests)"
exit 0
