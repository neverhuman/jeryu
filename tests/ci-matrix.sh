#!/usr/bin/env bash
# Exercise aggregation without starting product commands or fabricating evidence.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
source "$root/scripts/ci-lanes.sh"

hosted=$(sed -n 's/^        lane: \[\(.*\)\]$/\1/p' "$root/.github/workflows/ci.yml")
[[ ${hosted//, / } == "${JERYU_REQUIRED_CI_LANES[*]}" ]] || {
  printf 'Local and hosted required CI lanes differ\n' >&2; exit 1;
}
[[ " ${JERYU_REQUIRED_CI_LANES[*]} " == *' auxiliary '* &&
   " ${JERYU_REQUIRED_CI_LANES[*]} " != *' redline '* ]]

calls=()
mock_lane() {
  calls+=("$1")
  if [[ $1 == "$failure" ]]; then return "$failure_status"; fi
}
passed=0
for failure in none "${JERYU_REQUIRED_CI_LANES[@]}"; do
  for failure_status in 1 77 124; do
    calls=()
    result=0
    jeryu_ci_all mock_lane >/dev/null || result=$?
    [[ ${calls[*]} == "${JERYU_REQUIRED_CI_LANES[*]}" ]] || {
      printf 'CI omitted a lane after %s failed\n' "$failure" >&2; exit 1;
    }
    if [[ $failure == none ]]; then expected=0; else expected=1; fi
    [[ $result == "$expected" ]] || {
      printf 'Incorrect CI result after %s returned %s\n' "$failure" "$failure_status" >&2
      exit 1
    }
    passed=$((passed + 1))
  done
done
printf 'CI matrix and aggregation checks passed: %s scenarios\n' "$passed"
