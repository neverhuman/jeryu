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

# Exercise the actual Rust cases and shared Runner script with closed transports.
# Receipt data below is synthetic; no Cargo, auditor or kernel sandbox executes.
source "$root/tests/scratch.sh"
fixture=$(mktemp -d)
chmod 700 "$fixture"
jeryu_record_test_scratch "$fixture"
finish_coverage_fixture() {
  local status=$?
  trap - EXIT
  if [[ $status == 0 ]]; then
    jeryu_remove_test_scratch || exit 1
  else
    printf 'Retained failed CI dispatch fixture: %s\n' "$fixture" >&2
  fi
  exit "$status"
}
trap finish_coverage_fixture EXIT

# These are synthetic layouts, not source checkouts. Both contain exactly the
# maintained Runner helper bytes; no duplicate implementation lives in CI.
for layout_root in "$fixture" "$fixture/standalone"; do
  if [[ $layout_root == "$fixture" ]]; then
    helper_dir="$layout_root/components/jeryu-ci-runner/scripts"
  else
    helper_dir="$layout_root/scripts"
  fi
  mkdir -p "$helper_dir"
  printf '[workspace]\n' >"$layout_root/Cargo.toml"
  cat "$root/components/jeryu-ci-runner/scripts/test-native-sandbox.sh" >"$helper_dir/test-native-sandbox.sh"
done

fixture_call() {
  printf '%s\n' "$*" >>"$calls_file"
  [[ $* != "$fail_call" ]] || return 23
}
fixture_receipt_json() {
  printf '%s\n' '{"false_skips":0,"escapes":[{"verdict":"blocked"},{"verdict":"blocked"},{"verdict":"blocked"},{"verdict":"blocked"}]}'
}
fixture_cargo() {
  fixture_call cargo "$@" || return $?
  case "$*" in
    'fmt --all -- --check'|'clippy --locked --workspace --all-targets --all-features -- -D warnings'|'clippy --locked -p jeryu-api --all-targets --no-default-features -- -D warnings'|'fetch --locked') ;;
    'test --locked --workspace --all-features --exclude jeryu-sandbox-linux'|'test --locked -p jeryu-api --no-default-features')
      [[ ${fixture_auditor:-} == verified ]] || return 92
      ;;
    'run --locked -p jeryu-sandbox-linux --example required_capabilities') ;;
    'test --locked -p jeryu-sandbox-linux --all-features -- --include-ignored --nocapture --test-threads=1')
      if [[ $scenario == native-zero ]]; then
        printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n'
        return 0
      fi
      printf 'test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n'
      if [[ $scenario != stale-receipt && $scenario != missing-receipt ]]; then
        fixture_receipt_json >"$JERYU_SANDBOX_ENFORCEMENT_DIR/enforcement.json"
      fi
      case $scenario in
        stale-receipt|missing-producer) ;;
        prefixed-producer) printf 'extra enforcement receipt: %s/enforcement.json\n' "$JERYU_SANDBOX_ENFORCEMENT_DIR" ;;
        suffixed-producer) printf 'enforcement receipt: %s/enforcement.json extra\n' "$JERYU_SANDBOX_ENFORCEMENT_DIR" ;;
        *) printf '\nenforcement receipt: %s/enforcement.json\n' "$JERYU_SANDBOX_ENFORCEMENT_DIR" ;;
      esac
      if [[ $scenario == duplicate-producer ]]; then
        printf 'enforcement receipt: %s/enforcement.json\n' "$JERYU_SANDBOX_ENFORCEMENT_DIR"
      fi
      [[ $scenario != native-skip ]] || printf 'SKIP: unavailable kernel primitive\n'
      ;;
    'test --locked -p jeryu-agentbridge --test '* )
      local target=$6 count ignored=0 filtered=0
      [[ ! -v JERYU_SANDBOX_ENFORCEMENT_DIR ]] || return 97
      case "$*" in
        'test --locked -p jeryu-agentbridge --test driver_in_cell -- --nocapture --test-threads=1') count=4 ;;
        'test --locked -p jeryu-agentbridge --test pty_driver -- --nocapture --test-threads=1') count=3 ;;
        'test --locked -p jeryu-agentbridge --test cgroup_fail_closed -- --exact opt_out_driver_runs_on_this_no_delegation_host --nocapture --test-threads=1') count=1; filtered=1 ;;
        *) return 93 ;;
      esac
      if [[ $target == driver_in_cell ]]; then
        case $scenario in
          driver-skip) printf 'SKIP: landlock unavailable\n' ;;
          empty) count=0 ;;
          wrong-count) count=5 ;;
          ignored) count=3; ignored=1 ;;
          duplicate) printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n' ;;
          missing-summary) return 0 ;;
        esac
      elif [[ $target == pty_driver && $scenario == pty-skip ]]; then
        printf 'SKIP pty driver: sandbox unavailable\n'
      elif [[ $target == cgroup_fail_closed && $scenario == optout-skip ]]; then
        printf 'OPT-OUT honestly skipped (non-cgroup floor missing): unavailable\n'
      fi
      printf 'test result: ok. %s passed; 0 failed; %s ignored; 0 measured; %s filtered out; finished in 0.01s\n' "$count" "$ignored" "$filtered"
      ;;
    *) printf 'Unexpected Cargo invocation: %s\n' "$*" >&2; return 94 ;;
  esac
}
fixture_rg() {
  [[ $scenario != scan-error || $1 != -i ]] || return 2
  command rg "$@"
}
{
  printf '#!/usr/bin/env bash\nset -euo pipefail\nroot=$PWD\nscenario=$2\ncalls_file=$3\nfail_call=$4\ncomponent=${FIXTURE_COMPONENT:-jeryu-ci-runner}\nunset JERYU_SANDBOX_ENFORCEMENT_DIR\n'
  declare -f fixture_call fixture_receipt_json fixture_cargo fixture_rg
  cat <<'MOCKS'
cargo() { fixture_cargo "$@"; }
bash() {
  case $1 in
    components/jeryu-ci-runner/scripts/test-native-sandbox.sh|scripts/test-native-sandbox.sh) ;;
    *) return 96 ;;
  esac
  builtin source "$1" "${@:2}"
}
rg() { fixture_rg "$@"; }
web_build() { fixture_call web_build; }
source() { [[ $* == scripts/bootstrap-jankurai.sh ]] && fixture_call source "$@"; }
bootstrap_public_jankurai() {
  fixture_call bootstrap_public_jankurai || return $?
  fixture_auditor=verified
}
jq() {
  [[ $1 == -e && $2 == '.false_skips == 0 and (.escapes | length) == 4 and all(.escapes[]; .verdict == "blocked")' &&
     $3 == "$PWD/target/ci/sandbox-receipt."*/enforcement.json ]] || return 95
  fixture_call jq || return $?
  command jq "$@"
}
case $1 in
MOCKS
  sed -n '/^  rust)$/,/^    ;;$/p' "$root/scripts/ci.sh"
  sed -n '/^  sandbox)$/,/^    ;;$/p' "$root/scripts/ci.sh"
  sed -n '/^  sandbox)$/,/^    ;;$/p' "$root/components/jeryu-deploy/crates/jeryu-split-tool/src/split_ci.sh" |
    sed '1s/sandbox/split-sandbox/'
  printf '  shared) builtin source "$5" --workspace-root "$6" ;;\n  *) exit 96 ;;\nesac\n'
} >"$fixture/dispatch.sh"

run_coverage_case() {
  local lane=$1 scenario=$2 expected=$3 fail_call=${4:-} result=0
  local calls_file="$fixture/calls" transcript="$fixture/output"
  local case_root="$fixture"
  [[ $lane != split-sandbox ]] || case_root="$fixture/standalone"
  : >"$calls_file"
  (cd "$case_root" && JERYU_DISPOSABLE_SANDBOX=1 bash "$fixture/dispatch.sh" "$lane" "$scenario" "$calls_file" "$fail_call") >"$transcript" 2>&1 || result=$?
  [[ $result == "$expected" ]] || {
    printf 'CI dispatch %s/%s returned %s, expected %s\n' "$lane" "$scenario" "$result" "$expected" >&2
    cat "$transcript" >&2
    return 1
  }
  passed=$((passed + 1))
}
run_coverage_case rust normal 0
cat >"$fixture/expected" <<'CALLS'
web_build
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo fetch --locked
source scripts/bootstrap-jankurai.sh
bootstrap_public_jankurai
cargo test --locked --workspace --all-features --exclude jeryu-sandbox-linux
cargo test --locked -p jeryu-api --no-default-features
cargo clippy --locked -p jeryu-api --all-targets --no-default-features -- -D warnings
CALLS
cmp "$fixture/expected" "$fixture/calls"
while IFS= read -r fail_call; do
  run_coverage_case rust failure 23 "$fail_call"
  [[ $(tail -n 1 "$fixture/calls") == "$fail_call" ]]
done <"$fixture/expected"

# A cached green legacy receipt must never satisfy either invocation gate.
# Preserve the original 25 sandbox cases, and repeat them through split dispatch.
for lane in sandbox split-sandbox; do
  if [[ $lane == sandbox ]]; then
    case_root="$fixture"
    legacy_root="$fixture/components/jeryu-ci-runner"
  else
    case_root="$fixture/standalone"
    legacy_root="$case_root"
  fi
  mkdir -p "$legacy_root/target/jankurai/runner-sandbox"
  fixture_receipt_json >"$legacy_root/target/jankurai/runner-sandbox/enforcement.json"
  run_coverage_case "$lane" normal 0
  cat >"$fixture/expected" <<'CALLS'
cargo run --locked -p jeryu-sandbox-linux --example required_capabilities
cargo test --locked -p jeryu-sandbox-linux --all-features -- --include-ignored --nocapture --test-threads=1
cargo test --locked -p jeryu-agentbridge --test driver_in_cell -- --nocapture --test-threads=1
cargo test --locked -p jeryu-agentbridge --test pty_driver -- --nocapture --test-threads=1
cargo test --locked -p jeryu-agentbridge --test cgroup_fail_closed -- --exact opt_out_driver_runs_on_this_no_delegation_host --nocapture --test-threads=1
jq
CALLS
  cmp "$fixture/expected" "$fixture/calls"
  while IFS= read -r fail_call; do
    run_coverage_case "$lane" failure 23 "$fail_call"
    [[ $(tail -n 1 "$fixture/calls") == "$fail_call" ]]
  done <"$fixture/expected"
  for scenario in native-skip driver-skip pty-skip optout-skip empty wrong-count ignored duplicate missing-summary scan-error native-zero stale-receipt missing-receipt missing-producer duplicate-producer prefixed-producer suffixed-producer; do
    run_coverage_case "$lane" "$scenario" 1
    if rg -q '^jq$' "$fixture/calls"; then
      printf 'Rejected sandbox output reached receipt admission: %s/%s\n' "$lane" "$scenario" >&2
      exit 1
    fi
  done
  result=0
  : >"$fixture/calls"
  (cd "$case_root" && JERYU_DISPOSABLE_SANDBOX=0 bash "$fixture/dispatch.sh" "$lane" normal "$fixture/calls" '') >"$fixture/output" 2>&1 || result=$?
  [[ $result == 1 && ! -s $fixture/calls ]]
  passed=$((passed + 1))
done

FIXTURE_COMPONENT=jeryu-core run_coverage_case split-sandbox normal 1
[[ ! -s $fixture/calls ]]
mv "$fixture/standalone/scripts/test-native-sandbox.sh" "$fixture/standalone/scripts/retained-helper.sh"
run_coverage_case split-sandbox normal 1
[[ ! -s $fixture/calls ]]
mv "$fixture/standalone/scripts/retained-helper.sh" "$fixture/standalone/scripts/test-native-sandbox.sh"

# Direct calls cannot redirect the proof into another or non-physical workspace.
helper="$fixture/components/jeryu-ci-runner/scripts/test-native-sandbox.sh"
ln -s "$fixture" "$fixture/root-alias"
for selected_root in "$fixture/standalone" "$fixture/../$(basename -- "$fixture")" "$fixture/root-alias"; do
  result=0
  : >"$fixture/calls"
  JERYU_DISPOSABLE_SANDBOX=1 bash "$fixture/dispatch.sh" shared normal "$fixture/calls" '' "$helper" "$selected_root" >"$fixture/output" 2>&1 || result=$?
  [[ $result == 1 && ! -s $fixture/calls ]]
  passed=$((passed + 1))
done
[[ -L $fixture/root-alias && $(readlink -- "$fixture/root-alias") == "$fixture" ]]
unlink "$fixture/root-alias"
mv "$fixture/Cargo.toml" "$fixture/retained-manifest.toml"
run_coverage_case sandbox normal 1
[[ ! -s $fixture/calls ]]
ln -s "$fixture/retained-manifest.toml" "$fixture/Cargo.toml"
run_coverage_case sandbox normal 1
[[ ! -s $fixture/calls && -L $fixture/Cargo.toml &&
   $(readlink -- "$fixture/Cargo.toml") == "$fixture/retained-manifest.toml" ]]
unlink "$fixture/Cargo.toml"
mv "$fixture/retained-manifest.toml" "$fixture/Cargo.toml"
printf 'CI matrix and required dispatch checks passed: %s scenarios\n' "$passed"
