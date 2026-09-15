#!/usr/bin/env bash
# Keep the hosted matrices in sync with these lists; tests/ci-matrix.sh checks them.
# Required lanes live in `.github/workflows/ci.yml`. Advisory lanes live in
# `.github/workflows/nightly.yml` (schedule + dispatch). Local `scripts/ci.sh all`
# still runs the full union. GitHub `jeryu/required` aggregates only required lanes.
JERYU_HOSTED_REQUIRED_CI_LANES=(source public rust web runtime product security)
JERYU_HOSTED_ADVISORY_CI_LANES=(sandbox oci splits legacy auxiliary audit auditor)
JERYU_REQUIRED_CI_LANES=("${JERYU_HOSTED_REQUIRED_CI_LANES[@]}" "${JERYU_HOSTED_ADVISORY_CI_LANES[@]}")

jeryu_ci_all() {
  local lane result failed=0
  local -a results=()
  for lane in "${JERYU_REQUIRED_CI_LANES[@]}"; do
    if "$@" "$lane"; then result=0; else result=$?; fi
    results+=("$lane: exit=$result")
    [[ $result == 0 ]] || failed=1
  done
  printf '\nRequired CI results:\n'
  printf '%s\n' "${results[@]}"
  return "$failed"
}
