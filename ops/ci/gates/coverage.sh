#!/usr/bin/env bash
# GATE: coverage
# Engineering-spec phase: coverage + mutation evidence artifacts feeding the
# jankurai coverage audit (line_coverage `rust-lcov`, mutation `rust-mutation`
# in agent/coverage-sources.toml).
#
# Delegates to ops/ci/coverage.sh, which produces target/llvm-cov/lcov.info and
# target/mutants/mutants.out/outcomes.json, then runs `jankurai coverage audit`
# and asserts hard==0.
#
# Result policy (never silently green):
#   coverage.sh exit 0 -> GATE PASS  (artifacts produced; audit hard==0).
#   coverage.sh exit 3 -> GATE PENDING (a required external tool genuinely could
#                         not be installed on this host; skip-with-receipt was
#                         written). PENDING does not fail the run but is reported.
#   any other exit     -> GATE FAIL  (real failure: audit error or hard>0).
set -uo pipefail

GATE_NAME="coverage"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }

echo "[${GATE_NAME}] bash ops/ci/coverage.sh"
bash "${ROOT}/ops/ci/coverage.sh"
rc=$?

case "${rc}" in
  0)
    echo "GATE ${GATE_NAME}: PASS (lcov + mutants artifacts produced; jankurai coverage audit hard=0)"
    exit 0
    ;;
  3)
    echo "GATE ${GATE_NAME}: PENDING (coverage/mutation tooling unavailable on this host; skip-with-receipt at target/coverage/skip-receipt.txt)"
    exit 0
    ;;
  *)
    echo "GATE ${GATE_NAME}: FAIL (coverage lane failed; see output above, rc=${rc})"
    exit 1
    ;;
esac
