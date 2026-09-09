#!/usr/bin/env bash
# Thin local/hosted parity wrapper for the affected fast lane.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

source "${ROOT}/ops/ci/ci-env.sh"

if [ "${JERYU_CI_PROFILE}" = "local" ]; then
  export JERYU_CI_RUST_TEST_MODE="${JERYU_CI_RUST_TEST_MODE:-inline}"
else
  export JERYU_CI_RUST_TEST_MODE="${JERYU_CI_RUST_TEST_MODE:-sharded}"
fi

bash ci-fast-push.sh --no-push
