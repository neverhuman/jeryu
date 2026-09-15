#!/usr/bin/env bash
# Exercise aggregation without starting product commands or fabricating evidence.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
source "$root/scripts/ci-lanes.sh"

mapfile -t required_lines < <(sed -n 's/^        lane: \[\(.*\)\]$/\1/p' "$root/.github/workflows/ci.yml")
mapfile -t advisory_lines < <(sed -n 's/^        lane: \[\(.*\)\]$/\1/p' "$root/.github/workflows/nightly.yml")
[[ ${#required_lines[@]} == 1 && ${#advisory_lines[@]} == 1 ]] || {
  printf 'Hosted CI must declare one required matrix and one nightly advisory matrix\n' >&2; exit 1;
}
required_hosted=${required_lines[0]//, / }
advisory_hosted=${advisory_lines[0]//, / }
[[ $required_hosted == "${JERYU_HOSTED_REQUIRED_CI_LANES[*]}" ]] || {
  printf 'Local and hosted required CI lanes differ\n' >&2; exit 1;
}
[[ $advisory_hosted == "${JERYU_HOSTED_ADVISORY_CI_LANES[*]}" ]] || {
  printf 'Local and hosted advisory CI lanes differ\n' >&2; exit 1;
}
[[ ${JERYU_REQUIRED_CI_LANES[*]} == "${JERYU_HOSTED_REQUIRED_CI_LANES[*]} ${JERYU_HOSTED_ADVISORY_CI_LANES[*]}" ]] || {
  printf 'Required CI union does not match hosted required+advisory lanes\n' >&2; exit 1;
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
    'test --locked --workspace --all-features --exclude jeryu-sandbox-linux --no-fail-fast'|'test --locked -p jeryu-api --no-default-features --no-fail-fast')
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
    scripts/audit.sh|scripts/auxiliary-proofs.sh|ops/ci/score.sh)
      fixture_call bash "$@"
      return $?
      ;;
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
if [[ $1 == audit || $1 == auxiliary ]]; then set -- "$1"; fi
case $1 in
MOCKS
  sed -n '/^  audit)$/,/^    ;;$/p' "$root/scripts/ci.sh"
  sed -n '/^  auxiliary)$/,/^    ;;$/p' "$root/scripts/ci.sh"
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
cargo test --locked --workspace --all-features --exclude jeryu-sandbox-linux --no-fail-fast
cargo test --locked -p jeryu-api --no-default-features --no-fail-fast
cargo clippy --locked -p jeryu-api --all-targets --no-default-features -- -D warnings
CALLS
cmp "$fixture/expected" "$fixture/calls"
while IFS= read -r fail_call; do
  run_coverage_case rust failure 23 "$fail_call"
  case "$fail_call" in
    'cargo test '*|'cargo clippy --locked -p jeryu-api '*)
      cmp "$fixture/expected" "$fixture/calls"
      ;;
    *) [[ $(tail -n 1 "$fixture/calls") == "$fail_call" ]] ;;
  esac
done <"$fixture/expected"

# Hosted environment metadata cannot select a reduced required proof surface.
for environment in false true; do
  GITHUB_ACTIONS="$environment" run_coverage_case audit normal 0
  printf 'bash scripts/audit.sh\n' >"$fixture/expected"
  cmp "$fixture/expected" "$fixture/calls"
  GITHUB_ACTIONS="$environment" run_coverage_case audit failure 23 'bash scripts/audit.sh'
  GITHUB_ACTIONS="$environment" run_coverage_case auxiliary normal 0
  printf '%s\n' 'source scripts/bootstrap-jankurai.sh' 'bootstrap_public_jankurai' \
    'bash scripts/auxiliary-proofs.sh' >"$fixture/expected"
  cmp "$fixture/expected" "$fixture/calls"
  GITHUB_ACTIONS="$environment" run_coverage_case auxiliary failure 23 'bash scripts/auxiliary-proofs.sh'
done

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

# An independent legacy lane must populate its advisory cache before any owning
# cached audit. These synthetic transports exercise the real dispatch branch.
{
  printf '#!/usr/bin/env bash\nset -euo pipefail\nroot=$PWD\naudit_status=$1\ntools_status=$2\n'
  cat <<'LEGACY_MOCKS'
cargo() {
  printf 'cargo %s\n' "$*" >>calls
  [[ $# == 5 && $1 == audit && $2 == --deny && $3 == warnings &&
     $4 == --file && $5 == "$root/Cargo.lock" ]] || return 91
  [[ $audit_status == 0 ]] || return "$audit_status"
  printf 'fresh synthetic advisory database\n' >"$root/advisory-ready"
}
bash() {
  printf 'bash %s\n' "$*" >>"$root/calls"
  case "$*" in
    'scripts/bootstrap-ci-tools.sh --legacy') return "$tools_status" ;;
    'ops/ci/pr-ci.sh') [[ -f "$root/advisory-ready" ]] || return 92 ;;
    *) return 93 ;;
  esac
}
web_build() { printf 'web_build\n' >>calls; }
source() {
  [[ $* == scripts/bootstrap-jankurai.sh ]] || return 94
  printf 'source %s\n' "$*" >>calls
}
bootstrap_public_jankurai() { printf 'bootstrap_public_jankurai\n' >>calls; }
case legacy in
LEGACY_MOCKS
  sed -n '/^  legacy)$/,/^    ;;$/p' "$root/scripts/ci.sh"
  printf 'esac\n'
} >"$fixture/legacy-dispatch.sh"
for scenario in fresh unavailable-db advisory-finding unavailable-tool; do
  case_root="$fixture/legacy-$scenario"
  mkdir -p "$case_root/components/example"
  audit_status=0 tools_status=0 expected=0 result=0
  case $scenario in
    unavailable-db) audit_status=23; expected=23 ;;
    advisory-finding) audit_status=44; expected=44 ;;
    unavailable-tool) tools_status=45; expected=45 ;;
  esac
  (cd "$case_root" && bash "$fixture/legacy-dispatch.sh" "$audit_status" "$tools_status") >"$case_root/output" 2>&1 || result=$?
  [[ $result == "$expected" ]]
  if [[ $expected == 0 ]]; then
    [[ -f "$case_root/advisory-ready" &&
       $(grep -c '^bash ops/ci/pr-ci.sh$' "$case_root/calls") == 2 ]]
  else
    [[ ! -e "$case_root/advisory-ready" ]]
    if grep -qE '^(web_build|source |bootstrap_public_jankurai|bash ops/ci/pr-ci.sh)' "$case_root/calls"; then
      printf 'Legacy dispatch continued after a failed prerequisite\n' >&2
      exit 1
    else
      scan_status=$?
      [[ $scan_status == 1 ]] || exit "$scan_status"
    fi
  fi
  passed=$((passed + 1))
done

# Source admission needs all locked workspace/target inputs before offline metadata.
# Extract the actual branch; Cargo is synthetic and cannot fetch or compile here.
{
  printf '#!/usr/bin/env bash\nset -euo pipefail\nroot=$PWD\nscenario=$1\n'
  cat <<'SOURCE_MOCKS'
bash() {
  [[ $* == tests/ci-matrix.sh ]] || return 91
  printf 'bash %s\n' "$*" >>calls
  [[ $scenario != matrix-failure ]] || return 21
}
cargo() {
  printf 'cargo %s\n' "$*" >>calls
  case "$*" in
    'fetch --locked')
      [[ $scenario != fetch-failure ]] || return 23
      printf 'all synthetic locked inputs\n' >full-cache-ready ;;
    'run --locked -p jeryu-split-tool --bin jeryu-split -- monorepo-check')
      [[ -f full-cache-ready ]] || return 92
      [[ $scenario != check-failure ]] || return 24 ;;
    'run --locked -p jeryu-split-tool --bin jeryu-split -- manifest --check-paths'|\
    'run --locked -p jeryu-split-tool --bin jeryu-split -- proof-inventory --check')
      [[ -f full-cache-ready ]] || return 92 ;;
    *) return 93 ;;
  esac
}
case source in
SOURCE_MOCKS
  sed -n '/^  source)$/,/^    ;;$/p' "$root/scripts/ci.sh"
  printf 'esac\n'
} >"$fixture/source-dispatch.sh"
cat >"$fixture/source-expected" <<'SOURCE_CALLS'
bash tests/ci-matrix.sh
cargo fetch --locked
cargo run --locked -p jeryu-split-tool --bin jeryu-split -- monorepo-check
cargo run --locked -p jeryu-split-tool --bin jeryu-split -- manifest --check-paths
cargo run --locked -p jeryu-split-tool --bin jeryu-split -- proof-inventory --check
SOURCE_CALLS
for scenario in fresh fetch-failure check-failure matrix-failure; do
  case_root="$fixture/source-$scenario"
  mkdir "$case_root"
  expected=0 calls_count=5 result=0
  case $scenario in
    fetch-failure) expected=23; calls_count=2 ;;
    check-failure) expected=24; calls_count=3 ;;
    matrix-failure) expected=21; calls_count=1 ;;
  esac
  (cd "$case_root" && bash "$fixture/source-dispatch.sh" "$scenario") >"$case_root/output" 2>&1 || result=$?
  [[ $result == "$expected" ]]
  head -n "$calls_count" "$fixture/source-expected" >"$case_root/expected"
  cmp "$case_root/expected" "$case_root/calls"
  if [[ $scenario == fresh || $scenario == check-failure ]]; then
    [[ -f "$case_root/full-cache-ready" ]]
  else
    [[ ! -e "$case_root/full-cache-ready" ]]
  fi
  passed=$((passed + 1))
done
printf 'CI matrix and required dispatch checks passed: %s scenarios\n' "$passed"
