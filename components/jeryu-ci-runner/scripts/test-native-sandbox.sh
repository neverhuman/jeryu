#!/usr/bin/env bash
# One native proof command for monorepo CI and the generated Runner mirror.
set -euo pipefail
[[ $# == 2 && $1 == --workspace-root ]] || {
  printf 'usage: bash scripts/test-native-sandbox.sh --workspace-root PHYSICAL_ROOT\n' >&2
  exit 2
}
component_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
root=$2
# Admit only this component's standalone workspace or its monorepo workspace.
# No sibling repositories, ancestor search, or caller-selected proof source.
[[ $root == /* && -d $root && ! -L $root &&
   $(realpath -e -- "$root") == "$root" &&
   ( $component_root == "$root" || $component_root == "$root/components/jeryu-ci-runner" ) &&
   -f $root/Cargo.toml && ! -L $root/Cargo.toml ]] || {
  printf 'sandbox workspace must be the physical root owning this Runner source\n' >&2
  exit 1
}
cd -- "$root"
export CI=true
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}

# Run only in a disposable Linux environment with the required privileges.
[[ ${JERYU_DISPOSABLE_SANDBOX:-0} == 1 ]] || {
  printf 'sandbox lane requires JERYU_DISPOSABLE_SANDBOX=1 on a disposable Linux host\n' >&2; exit 1;
}
mkdir -p target/ci
cargo run --locked -p jeryu-sandbox-linux --example required_capabilities
sandbox_receipt_dir=$(mktemp -d "$root/target/ci/sandbox-receipt.XXXXXXXX")
[[ -O $sandbox_receipt_dir && ! -L $sandbox_receipt_dir &&
   $(realpath -e -- "$sandbox_receipt_dir") == "$sandbox_receipt_dir" ]] || exit 1
JERYU_SANDBOX_ENFORCEMENT_DIR="$sandbox_receipt_dir" \
  cargo test --locked -p jeryu-sandbox-linux --all-features -- --include-ignored --nocapture --test-threads=1 2>&1 | tee target/ci/sandbox.log
# Only the escape producer writes here. A previous cached receipt cannot
# satisfy this invocation, and zero executed producer tests cannot pass.
[[ $(rg -F -x -c -- "enforcement receipt: $sandbox_receipt_dir/enforcement.json" target/ci/sandbox.log) == 1 &&
   -f $sandbox_receipt_dir/enforcement.json && ! -L $sandbox_receipt_dir/enforcement.json ]] || {
  printf 'sandbox escape producer did not publish this invocation receipt\n' >&2; exit 1
}
# Ordinary hosts may return early from these eight tests. Require their
# actual execution here; the paid external-model smoke remains separate.
sandbox_logs=(target/ci/sandbox.log)
for test_target in driver_in_cell pty_driver cgroup_fail_closed; do
  test_filter=()
  filtered=0
  case $test_target in
    driver_in_cell) expected_tests=4 ;;
    pty_driver) expected_tests=3 ;;
    cgroup_fail_closed)
      expected_tests=1
      filtered=1
      test_filter=(--exact opt_out_driver_runs_on_this_no_delegation_host)
      ;;
  esac
  test_log="target/ci/agentbridge-$test_target.log"
  cargo test --locked -p jeryu-agentbridge --test "$test_target" -- \
    "${test_filter[@]}" --nocapture --test-threads=1 2>&1 | tee "$test_log"
  [[ $(rg -c '^test result:' "$test_log") == 1 ]] &&
    rg -q "^test result: ok\. $expected_tests passed; 0 failed; 0 ignored; 0 measured; $filtered filtered out;" "$test_log" || {
      printf 'Agentbridge %s did not execute its %s required cases\n' "$test_target" "$expected_tests" >&2
      exit 1
    }
  sandbox_logs+=("$test_log")
done
skip_status=0
rg -i '(^|[[:space:]])skip[:[:space:]]|skipping|=> skipped|honestly skipped|"skipped"[[:space:]]*:[[:space:]]*[1-9]|[1-9][0-9]* ignored' "${sandbox_logs[@]}" || skip_status=$?
[[ $skip_status == 1 ]] || {
  printf 'sandbox proof was skipped or its output could not be checked\n' >&2; exit 1
}
jq -e '.false_skips == 0 and (.escapes | length) == 4 and all(.escapes[]; .verdict == "blocked")' \
  "$sandbox_receipt_dir/enforcement.json" >/dev/null
