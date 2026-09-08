#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
ci_script="$repo_root/scripts/ci-local.sh"
real_bash="$(command -v bash)"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-tool-finder-ci-dispatch.XXXXXX")"
shim_dir="$test_root/bin"
foreign_cwd="$test_root/foreign-cwd"
dispatch_log="$test_root/dispatch.log"
stdout_log="$test_root/stdout.log"
stderr_log="$test_root/stderr.log"
injection_marker="$test_root/injection-ran"

cleanup() {
  rm -rf -- "$test_root"
}
trap cleanup EXIT

mkdir -p -- "$shim_dir" "$foreign_cwd"

# These expressions are evaluated by the generated shim, not by this test.
# shellcheck disable=SC2016
{
  printf '%s\n' '#!/bin/sh'
  printf '%s\n' 'printf "cwd=%s\targc=%s" "$PWD" "$#" >> "$CI_DISPATCH_LOG"'
  printf '%s\n' 'for argument in "$@"; do'
  printf '%s\n' '  printf "\t%s" "$argument" >> "$CI_DISPATCH_LOG"'
  printf '%s\n' 'done'
  printf '%s\n' 'printf "\n" >> "$CI_DISPATCH_LOG"'
  printf '%s\n' 'exit "${CI_DISPATCH_EXIT:-0}"'
} > "$shim_dir/bash"
chmod 0755 "$shim_dir/bash"

fail() {
  printf 'ci-local dispatch test failed: %s\n' "$1" >&2
  sed -n '1,20p' "$dispatch_log" >&2
  sed -n '1,20p' "$stderr_log" >&2
  exit 1
}

run_ci() {
  : > "$dispatch_log"
  : > "$stdout_log"
  : > "$stderr_log"

  set +e
  (
    cd "$foreign_cwd"
    PATH="$shim_dir:$PATH" CI_DISPATCH_LOG="$dispatch_log" \
      CI_DISPATCH_EXIT="${ci_dispatch_exit:-0}" \
      "$real_bash" "$ci_script" "$@"
  ) > "$stdout_log" 2> "$stderr_log"
  run_status=$?
  set -e
}

assert_status() {
  local expected="$1"
  [[ "$run_status" -eq "$expected" ]] ||
    fail "expected status $expected, got $run_status"
}

assert_dispatch() {
  local lane="$1"
  local relative_script="$2"
  local expected

  run_ci "$lane"
  assert_status 0
  [[ "$(wc -l < "$dispatch_log")" -eq 1 ]] ||
    fail "$lane did not delegate exactly once"
  printf -v expected 'cwd=%s\targc=1\t%s' \
    "$repo_root" "$repo_root/$relative_script"
  [[ "$(< "$dispatch_log")" == "$expected" ]] ||
    fail "$lane delegated with the wrong root, argument count, or script"
}

assert_rejected() {
  run_ci "$@"
  assert_status 2
  [[ ! -s "$dispatch_log" ]] || fail 'rejected input delegated a command'
}

assert_dispatch required ops/ci/pr-ci.sh
assert_dispatch security tools/security-lane.sh
assert_dispatch score ops/ci/score.sh
assert_dispatch contract-drift ops/ci/contract-drift.sh
assert_dispatch artifact-support ops/ci/artifact-support.sh

ci_dispatch_exit=37
run_ci security
assert_status 37
expected_failure="cwd=$repo_root"$'\targc=1\t'"$repo_root/tools/security-lane.sh"
[[ "$(< "$dispatch_log")" == "$expected_failure" ]] ||
  fail 'delegated failure changed command identity'
ci_dispatch_exit=0

assert_rejected
assert_rejected required extra
assert_rejected unknown
assert_rejected fast
assert_rejected check
assert_rejected --help
assert_rejected -x
assert_rejected "required; touch $injection_marker"
# This is a literal hostile argument, not an expression for this test shell.
# shellcheck disable=SC2016
assert_rejected '$(touch '"$injection_marker"')'
assert_rejected $'required\nsecurity'
[[ ! -e "$injection_marker" ]] || fail 'injection-shaped input created a file'

printf 'ci-local dispatch tests ok\n'
