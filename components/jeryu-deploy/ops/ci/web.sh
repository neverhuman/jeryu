#!/usr/bin/env bash
# Prove Deploy's admitted production bundle through its actual CLI process tests.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
cd "$ROOT"
source "$ROOT/ops/ci/common.sh"
# shellcheck source=ops/ci/web-build.sh
source "$ROOT/ops/ci/web-build.sh"
finish_web() {
  local result=$?
  trap - EXIT
  jeryu_web_finish "$result" || { if (( result == 0 )); then result=1; fi; }
  exit "$result"
}
trap finish_web EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
jeryu_web_begin "$ROOT"
# Retain the browser-vs-JSON routing unit regression, with its explicit fixture.
cargo test --locked -p jeryu-api --features web --jobs "$JERYU_CI_JOBS" \
  browser_repo_routes_serve_the_spa_shell
# These three tests execute this Cargo package's binary, never an installed/root override.
env -u JERYU_TEST_BINARY cargo test --locked -p jeryu-cli --test standalone --jobs "$JERYU_CI_JOBS"
jeryu_web_finish 0
trap - EXIT
printf 'web gate: production bundle, three CLI process tests and source/bundle readback passed\n'
