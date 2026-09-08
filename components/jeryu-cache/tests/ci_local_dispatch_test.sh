#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ci_script="$repo_root/scripts/ci-local.sh"
real_bash="$(command -v bash)"
test_root="$(mktemp -d)"
shim_dir="$test_root/bin"
dispatch_log="$test_root/dispatch.log"
stdout_log="$test_root/stdout.log"
stderr_log="$test_root/stderr.log"

cleanup() {
  rm -rf -- "$test_root"
}
trap cleanup EXIT

mkdir -p "$shim_dir"

make_shim() {
  local command_name="$1"
  local shim="$shim_dir/$command_name"

  {
    printf '%s\n' '#!/bin/sh'
    printf 'command_name=%s\n' "$command_name"
    printf '%s\n' '{'
    printf '%s\n' "  printf \"%s\" \"\$command_name\""
    printf '%s\n' '  for argument do'
    printf '%s\n' "    printf \"\\\\t%s\" \"\$argument\""
    printf '%s\n' '  done'
    printf '%s\n' '  printf "\\n"'
    printf '%s\n' "} >> \"\$CI_DISPATCH_LOG\""
    # This is the generated shim's runtime expression.
    # shellcheck disable=SC2016
    printf '%s\n' 'exit "${CI_DISPATCH_EXIT:-0}"'
  } > "$shim"
  chmod 0755 "$shim"
}

# `bash` is the intended dispatcher. `just` catches the old unconditional
# `just fast; just check` fallback if it ever returns.
make_shim bash
make_shim just

run_ci() {
  : > "$dispatch_log"
  : > "$stdout_log"
  : > "$stderr_log"

  set +e
  PATH="$shim_dir:$PATH" CI_DISPATCH_LOG="$dispatch_log" \
    CI_DISPATCH_EXIT="${ci_dispatch_exit:-0}" \
    "$real_bash" "$ci_script" "$@" > "$stdout_log" 2> "$stderr_log"
  run_status=$?
  set -e
}

fail() {
  printf 'ci-local dispatch test failed: %s\n' "$1" >&2
  printf '%s\n' '--- dispatch log ---' >&2
  sed -n '1,40p' "$dispatch_log" >&2
  printf '%s\n' '--- stderr ---' >&2
  sed -n '1,40p' "$stderr_log" >&2
  exit 1
}

assert_status() {
  local expected="$1"
  [ "$run_status" -eq "$expected" ] || \
    fail "expected status $expected, got $run_status"
}

assert_dispatch() {
  local lane="$1"
  local expected_script="$2"
  local expected
  expected="$(printf 'bash\t%s' "$expected_script")"

  run_ci "$lane"
  assert_status 0
  [ "$(wc -l < "$dispatch_log")" -eq 1 ] || \
    fail "$lane did not execute exactly one command"
  [ "$(sed -n '1p' "$dispatch_log")" = "$expected" ] || \
    fail "$lane dispatched the wrong command"
}

assert_no_dispatch() {
  [ ! -s "$dispatch_log" ] || fail 'rejected input executed a command'
}

assert_stderr_line() {
  local expected="$1"
  [ "$(sed -n '1p' "$stderr_log")" = "$expected" ] || \
    fail 'stderr did not report the expected causal error'
  [ "$(wc -l < "$stderr_log")" -eq 1 ] || \
    fail 'stderr contained unexpected additional output'
}

assert_dispatch required ops/ci/pr-ci.sh
assert_dispatch security tools/security-lane.sh
assert_dispatch score ops/ci/score.sh
assert_dispatch contract-drift ops/ci/contract-drift.sh
assert_dispatch artifact-support ops/ci/artifact_support.sh

ci_dispatch_exit=37
run_ci contract-drift
assert_status 37
[ "$(wc -l < "$dispatch_log")" -eq 1 ] || \
  fail 'contract-drift failure did not execute exactly one delegated command'
[ "$(sed -n '1p' "$dispatch_log")" = \
  $'bash\tops/ci/contract-drift.sh' ] || \
  fail 'contract-drift failure did not preserve delegated identity'
ci_dispatch_exit=0

run_ci
assert_status 2
assert_no_dispatch
assert_stderr_line \
  'usage: ci-local.sh {required|security|score|contract-drift|artifact-support}'

run_ci required extra
assert_status 2
assert_no_dispatch
assert_stderr_line \
  'usage: ci-local.sh {required|security|score|contract-drift|artifact-support}'

run_ci unknown
assert_status 2
assert_no_dispatch
assert_stderr_line 'unsupported CI lane: unknown'

run_ci fast
assert_status 2
assert_no_dispatch
assert_stderr_line 'unsupported CI lane: fast'

run_ci check
assert_status 2
assert_no_dispatch
assert_stderr_line 'unsupported CI lane: check'

run_ci 'required; just check'
assert_status 2
assert_no_dispatch
assert_stderr_line 'unsupported CI lane: required; just check'

printf 'ci-local dispatch tests ok\n'
