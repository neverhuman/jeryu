#!/usr/bin/env bash
# Pure dispatch/status contracts; no real compiler, auditor or CI lane runs.
set -euo pipefail
(( $# <= 2 )) || { printf 'usage: component-ci-dispatch.sh [ROOT [CANDIDATE_SCRIPT]]\n' >&2; exit 2; }
root=${1:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)}
candidate=${2:-}
# The optional individual script is for artifact review before source integration.
# Normal CI exercises the five quick/default dispatchers and Release Ops.
# shellcheck source=tests/scratch.sh
source "$root/tests/scratch.sh"
bash_bin=$(command -v bash)
umask 077
temporary=$(mktemp -d -t jeryu-component-dispatch.XXXXXXXX)
jeryu_record_test_scratch "$temporary"
cleanup() {
  local result=$?
  if (( result != 0 )); then
    printf 'retaining failed dispatch fixtures: %s\n' "$temporary" >&2
    return "$result"
  fi
  if ! jeryu_remove_test_scratch; then
    printf 'retaining changed, linked or mounted dispatch fixtures: %s\n' "$temporary" >&2
    return 1
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$temporary/bin" "$temporary/foreign cwd"
cat > "$temporary/bin/just" <<'MOCK'
#!/bin/bash
set -euo pipefail
[[ $# == 1 && $PWD == "$EXPECTED_DEFAULT_CWD" ]] || exit 91
case $1 in
  fast) printf 'fast\n' >> "$TRACE"; exit "$FAST_STATUS" ;;
  check) printf 'check\n' >> "$TRACE"; exit "$CHECK_STATUS" ;;
  score|security|artifact-support|redline-consumer-test) printf '%s\n' "$1" >> "$TRACE"; exit "$CHECK_STATUS" ;;
  *) exit 92 ;;
esac
MOCK
cat > "$temporary/bin/bash" <<'MOCK'
#!/bin/bash
set -euo pipefail
[[ $# == 1 && $1 == ops/ci/pr-ci.sh && $PWD == "$EXPECTED_REQUIRED_CWD" ]] || exit 93
printf 'required\n' >> "$TRACE"
printf 'SYNTHETIC PASS marker; exit status still controls this fixture\n'
exit "$REQUIRED_STATUS"
MOCK
chmod 0700 "$temporary/bin/just" "$temporary/bin/bash"
passed=0
run_case() {
  local name=$1 expected_status=$2 expected_trace=$3 observed=0
  shift 3
  : > "$temporary/trace"
  (
    cd "$temporary/foreign cwd"
    env -i PATH="$temporary/bin:/usr/bin:/bin" HOME="$temporary" \
      TRACE="$temporary/trace" EXPECTED_DEFAULT_CWD="${default_cwd:-$temporary/foreign cwd}" \
      EXPECTED_REQUIRED_CWD="$component_root" \
      FAST_STATUS="${fast_status:-0}" CHECK_STATUS="${check_status:-0}" \
      REQUIRED_STATUS="${required_status:-0}" \
      "$bash_bin" "$dispatcher" "$@"
  ) > "$temporary/output" 2>&1 || observed=$?
  if [[ $observed != "$expected_status" ||
        $(<"$temporary/trace") != "$expected_trace" ]]; then
    printf 'dispatch mismatch: %s/%s expected=%s observed=%s\n' \
      "$component" "$name" "$expected_status" "$observed" >&2
    return 1
  fi
  passed=$((passed + 1))
}
for component in jeryu-ci-runner jeryu-core jeryu-deploy jeryu-intelligence jeryu-tool; do
  component_root="$root/components/$component"
  dispatcher="$component_root/scripts/ci-local.sh"
  if [[ -n $candidate ]]; then
    dispatcher=$candidate
    component_root=$(cd -- "$(dirname -- "$candidate")/.." && pwd -P)
  fi
  fast_status=0 check_status=0 required_status=0
  run_case default 0 $'fast\ncheck'
  fast_status=17
  run_case fast_failure 17 fast
  fast_status=0 check_status=19
  run_case check_failure 19 $'fast\ncheck'
  fast_status=97 check_status=98
  run_case required_no_quick_fallback 0 required required
  for required_status in 3 23 143; do
    run_case required_failure "$required_status" required required
  done
  required_status=0
  run_case empty_argument 2 '' ''
  run_case unknown_argument 2 '' fast
  run_case near_match 2 '' 'required '
  run_case help_is_unsupported 2 '' --help
  run_case extra_argument 2 '' required extra
  run_case extra_empty_argument 2 '' required ''
done
# Release Ops preserves its established default of the complete required gate.
# Its other named lanes continue to select exactly one existing recipe.
if [[ -z $candidate ]]; then
  component=jeryu-release-ops
  component_root="$root/components/$component"
  dispatcher="$component_root/scripts/ci-local.sh"
  default_cwd=$component_root
  fast_status=0 check_status=0 required_status=0
  run_case default 0 required
  run_case required 0 required required
  for required_status in 3 23 143; do
    run_case required_failure "$required_status" required required
  done
  required_status=0
  for lane in fast check score security artifact-support; do run_case "$lane" 0 "$lane" "$lane"; done
  run_case contract_drift 0 redline-consumer-test contract-drift
  run_case unknown_argument 2 '' unknown
  run_case empty_argument 2 '' ''
  run_case extra_argument 2 '' required extra
  run_case extra_empty_argument 2 '' required ''
fi
# Report success only after the shared identity/link/mount cleanup succeeds.
jeryu_remove_test_scratch
trap - EXIT
printf 'component CI dispatch: %s synthetic cases passed; no real CI executed\n' "$passed"
