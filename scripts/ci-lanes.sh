#!/usr/bin/env bash
# Keep the hosted matrix in sync with this list; tests/ci-matrix.sh checks it.
JERYU_REQUIRED_CI_LANES=(source public rust web runtime product security sandbox oci splits legacy auxiliary audit auditor)

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
