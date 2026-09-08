#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/ops/ci/pr-ci.sh"

fail() {
  printf 'pr-ci order test failed: %s\n' "$*" >&2
  exit 1
}

line_of() {
  local exact="$1"
  grep -Fxn -- "$exact" "$script" | cut -d: -f1
}

require_once() {
  local exact="$1" line
  line="$(line_of "$exact")"
  [[ "$line" =~ ^[1-9][0-9]*$ ]] \
    || fail "expected exactly one command: $exact"
  printf '%s\n' "$line"
}

workspace_line="$(require_once 'cargo nextest run --locked --offline --workspace \')"
poisoning_line="$(require_once 'bash tests/cache_poisoning_matrix.sh')"
contract_line="$(require_once 'bash ops/ci/contract-drift.sh')"
artifact_line="$(require_once 'bash ops/ci/artifact_support.sh')"
security_hostile_line="$(require_once 'bash tests/security_lane_test.sh')"
contract_hostile_line="$(require_once 'bash tests/contract_drift_test.sh')"
artifact_hostile_line="$(require_once 'bash tests/artifact_support_test.sh')"
contract_final_line="$(require_once 'bash ops/ci/contract-drift.sh --validate-receipt \')"
artifact_final_line="$(require_once 'bash ops/ci/artifact_support.sh --validate-receipt \')"
ok_line="$(require_once 'echo "[pr-ci] jeryu-cache OK" >&2')"

[[ "$workspace_line" -lt "$poisoning_line" \
  && "$poisoning_line" -lt "$contract_line" \
  && "$contract_line" -lt "$artifact_line" \
  && "$artifact_line" -lt "$security_hostile_line" \
  && "$security_hostile_line" -lt "$contract_hostile_line" \
  && "$contract_hostile_line" -lt "$artifact_hostile_line" \
  && "$artifact_hostile_line" -lt "$contract_final_line" \
  && "$contract_final_line" -lt "$artifact_final_line" \
  && "$artifact_final_line" -lt "$ok_line" ]] \
  || fail 'product execution, evidence generation, hostiles, and final revalidation are out of order'

printf 'pr-ci evidence ordering ok\n'
