#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

run_local_state() {
  local mode="$1" ss_fixture="$2" ps_fixture="$3" summary="$4"
  JERYU_CI_ALLOW_RETIRED_REMOTES=1 \
  JERYU_CI_ALLOW_RETIRED_SOURCE_ROOTS=1 \
  JERYU_LOCAL_STATE_SS_FIXTURE="${ss_fixture}" \
  JERYU_LOCAL_STATE_PS_FIXTURE="${ps_fixture}" \
    bash ops/ci/local-state.sh "${mode}" --summary "${summary}"
}

assert_jq() {
  local summary="$1" filter="$2"
  jq -e "${filter}" "${summary}" >/dev/null
}

safe_summary="${TMP_DIR}/safe.json"
run_local_state --repair \
  tests/fixtures/local-state/safe-repo-api.ss \
  tests/fixtures/local-state/safe-repo-api.ps \
  "${safe_summary}"
assert_jq "${safe_summary}" '.ok == true'
assert_jq "${safe_summary}" '.auto_stopped | length == 1'
assert_jq "${safe_summary}" '.auto_stopped[0].pid == 4242'
assert_jq "${safe_summary}" '.auto_stopped[0].bind_address == "127.0.0.1:8787"'
assert_jq "${safe_summary}" '.release_posture == "release_validation_idle"'

unsafe_summary="${TMP_DIR}/unsafe.json"
if run_local_state --verify \
  tests/fixtures/local-state/unsafe-external-listener.ss \
  tests/fixtures/local-state/unsafe-external-listener.ps \
  "${unsafe_summary}"; then
  echo "expected unsafe external listener to block verification" >&2
  exit 1
fi
assert_jq "${unsafe_summary}" '.ok == false'
assert_jq "${unsafe_summary}" '.blockers[0].kind == "unknown_listener"'
assert_jq "${unsafe_summary}" '.blockers[0].bind_address == "127.0.0.1:8787"'

retired_data_summary="${TMP_DIR}/retired-data.json"
run_local_state --repair \
  tests/fixtures/local-state/empty.ss \
  tests/fixtures/local-state/retired-data-api.ps \
  "${retired_data_summary}"
assert_jq "${retired_data_summary}" '.ok == true'
assert_jq "${retired_data_summary}" '.auto_stopped | length == 1'
assert_jq "${retired_data_summary}" '.auto_stopped[0].reason == "repo_owned_api_retired_data_dir"'
assert_jq "${retired_data_summary}" '.auto_stopped[0].bind_address == "not_listening"'

no_runner_summary="${TMP_DIR}/no-runner.json"
run_local_state --verify \
  tests/fixtures/local-state/empty.ss \
  tests/fixtures/local-state/no-runner.ps \
  "${no_runner_summary}"
assert_jq "${no_runner_summary}" '.ok == true'
assert_jq "${no_runner_summary}" '.runner_posture.live_runner_required == false'
assert_jq "${no_runner_summary}" '.runner_posture.live_runner_state == "absent"'
assert_jq "${no_runner_summary}" '.runner_posture.proof.deterministic_slots == 40'

echo "local state fixture tests: PASS"
