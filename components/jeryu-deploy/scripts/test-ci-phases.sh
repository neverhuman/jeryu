#!/usr/bin/env bash
# Exercise the actual dispatchers using synthetic child commands, without CI evidence.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
umask 077
scratch=$(mktemp -d /tmp/jeryu-phase-status.XXXXXXXX)
identity=$(stat -c '%d:%i:%u:%g:%a' -- "$scratch")
cleanup() {
  local status=$? mount_point unexpected
  if [[ ! -d $scratch || -L $scratch || $(realpath -e -- "$scratch") != "$scratch" ||
        $(stat -c '%d:%i:%u:%g:%a' -- "$scratch") != "$identity" ]]; then
    printf 'phase fixture custody changed; retained %s\n' "$scratch" >&2
    exit 1
  fi
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "$mount_point"
    [[ $mount_point != "$scratch" && $mount_point != "$scratch/"* ]] || exit 1
  done </proc/self/mountinfo
  unexpected=$(find -P "$scratch" -xdev \( -type l -o \( -type f ! -links 1 \) -o \( ! -type d ! -type f \) \) -print -quit) || exit 1
  if [[ -n $unexpected ]]; then
    printf 'phase fixture contains an unexpected file or link; retained %s\n' "$scratch" >&2
    exit 1
  fi
  rm -rf --one-file-system --preserve-root=all -- "$scratch" || exit 1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$scratch/scripts" "$scratch/ops/ci/gates"
# Two programs under test are installed into a minimal synthetic fixture.
install -m 600 "$root/scripts/ci-phases.sh" "$scratch/scripts/ci-phases.sh"
install -m 600 "$root/ops/ci/gates/coverage.sh" "$scratch/coverage-wrapper.sh"
cases=0
require_status() {
  local expected=$1 actual=0
  shift
  "$@" >"$scratch/output" 2>&1 || actual=$?
  [[ $actual == "$expected" ]] || {
    cat "$scratch/output" >&2
    printf 'expected exit %s, got %s: %s\n' "$expected" "$actual" "$*" >&2
    exit 1
  }
  cases=$((cases + 1))
}
gate_case() {
  local expected=$1 rc=$2 message=$3
  printf 'printf "%%s\\n" %q\nexit %q\n' "$message" "$rc" >"$scratch/ops/ci/gates/subject.sh"
  require_status "$expected" bash "$scratch/scripts/ci-phases.sh"
}
gate_case 0 0 'GATE subject: PASS'
gate_case 0 0 'GATE subject: PASS (actual child completed)'
gate_case 1 0 'GATE subject: FAIL'
gate_case 1 0 'GATE subject: PENDING'
gate_case 1 3 'GATE subject: PENDING'
gate_case 1 1 'GATE subject: PASS'
gate_case 1 137 'GATE subject: PASS'
gate_case 1 0 'GATE another: PASS'
gate_case 1 0 $'GATE subject: PASS\nlater output'
gate_case 1 0 ''
gate_case 1 0 'GATE subject: PASSAGE'
printf 'printf "GATE zlater: PASS\\n"\n' >"$scratch/ops/ci/gates/zlater.sh"
gate_case 1 1 'GATE subject: FAIL'
[[ $(cat "$scratch/output") == *'GATE zlater: PASS'* ]]
require_status 0 bash "$scratch/scripts/ci-phases.sh" --list
[[ $(cat "$scratch/output") != *'GATE zlater: PASS'* ]]
require_status 2 bash "$scratch/scripts/ci-phases.sh" --skip
rm -- "$scratch/ops/ci/gates/subject.sh" "$scratch/ops/ci/gates/zlater.sh"
require_status 1 bash "$scratch/scripts/ci-phases.sh"
install -m 600 "$scratch/coverage-wrapper.sh" "$scratch/ops/ci/gates/coverage.sh"
for rc in 0 1 3 125; do
  printf 'exit %q\n' "$rc" >"$scratch/ops/ci/coverage.sh"
  expected=1
  [[ $rc != 0 ]] || expected=0
  [[ $rc != 3 ]] || expected=3
  require_status "$expected" bash "$scratch/ops/ci/gates/coverage.sh"
  if [[ $rc == 0 ]]; then
    [[ $(cat "$scratch/output") == *'GATE coverage: PASS '* ]]
  else
    [[ $(cat "$scratch/output") == *'GATE coverage: FAIL '* ]]
  fi
done
printf '%s phase-dispatch/coverage failure propagation cases passed\n' "$cases"
