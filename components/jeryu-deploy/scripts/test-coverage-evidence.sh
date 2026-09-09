#!/usr/bin/env bash
# Synthetic inputs exercise real coverage guards; no coverage/auditor is run.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
ratchet=${1:-$root/ops/ci/coverage_ratchet.sh}
mutation=${2:-$root/ops/ci/coverage-mutants.sh}
umask 077
scratch=$(mktemp -d /tmp/jeryu-coverage-fixture.XXXXXXXX)
identity=$(stat -c '%d:%i:%u:%g:%a' -- "$scratch")
cleanup() {
  local status=$? mount_point unexpected
  if [[ ! -d $scratch || -L $scratch || $(realpath -e -- "$scratch") != "$scratch" ||
        $(stat -c '%d:%i:%u:%g:%a' -- "$scratch") != "$identity" ]]; then
    printf 'coverage fixture custody changed; retained %s\n' "$scratch" >&2; exit 1
  fi
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "$mount_point"
    [[ $mount_point != "$scratch" && $mount_point != "$scratch/"* ]] || exit 1
  done </proc/self/mountinfo
  unexpected=$(find -P "$scratch" -xdev \( -type l -o \( -type f ! -links 1 \) -o \( ! -type d ! -type f \) \) -print -quit) || exit 1
  [[ -z $unexpected ]] || { printf 'unexpected fixture entry; retained %s\n' "$scratch" >&2; exit 1; }
  rm -rf --one-file-system --preserve-root=all -- "$scratch" || exit 1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
cases=0
assert_exit() {
  local expected=$1 actual=0
  shift
  "$@" >"$scratch/output" 2>&1 || actual=$?
  [[ $actual == "$expected" ]] || {
    cat "$scratch/output" >&2
    printf 'expected exit %s, got %s: %s\n' "$expected" "$actual" "$*" >&2; exit 1
  }
  cases=$((cases + 1))
}
baseline() { printf 'jeryu-api\t0.8044\n' >"$scratch/baseline"; }
lcov() {
  awk -v hits="$1" -v lines="${2:-10000}" 'BEGIN {
    print "SF:/clearly-test-only/crates/jeryu-api/src/lib.rs"
    for (i = 1; i <= lines; i++) print "DA:" i "," (i <= hits ? 1 : 0)
    print "LF:" lines "\nLH:" hits "\nend_of_record"
  }' >"$scratch/lcov"
}
check() { bash "$ratchet" "$scratch/lcov" "$scratch/baseline" jeryu-api; }
export JERYU_COVERAGE_UPDATE_BASELINE=0 JERYU_COVERAGE_EPSILON=0.005
baseline
lcov 8044; assert_exit 0 check
lcov 7994; assert_exit 0 check
lcov 7993; assert_exit 1 check
lcov 15987 20000; assert_exit 1 check
lcov 15988 20000; assert_exit 0 check
lcov 9000
mv "$scratch/baseline" "$scratch/saved-baseline"
assert_exit 1 check
[[ ! -e $scratch/baseline ]]
mv "$scratch/saved-baseline" "$scratch/baseline"
sed 's,jeryu-api/src,unrelated/src,' "$scratch/lcov" >"$scratch/absent-lcov"
assert_exit 1 bash "$ratchet" "$scratch/absent-lcov" "$scratch/baseline" jeryu-api
assert_exit 1 bash "$ratchet" "$scratch/lcov" "$scratch/baseline" jeryu-api missing
assert_exit 1 bash "$ratchet" "$scratch/lcov" "$scratch/baseline" jeryu-api jeryu-api
for row in $'jeryu-api\tNaN' $'jeryu-api\t-0.1' $'jeryu-api\t1.1' $'jeryu-api\t0.8\textra' $'jeryu-api\t0.8\njeryu-api\t0.8' $'other\t0.8'; do
  printf '%s\n' "$row" >"$scratch/baseline"
  assert_exit 1 check
done
baseline
for edit in '/^DA:1,/p' '/^LF:/p' '/^end_of_record$/d' 's/LF:10000/LF:9999/' 's/LH:9000/LH:10001/' 's/LF:10000/LF:10000:0/' 's/DA:1,1/DA:1,bad/' 's/LF:10000/unknown:1/'; do
  lcov 9000
  sed "$edit" "$scratch/lcov" >"$scratch/bad-lcov"
  assert_exit 1 bash "$ratchet" "$scratch/bad-lcov" "$scratch/baseline" jeryu-api
done
lcov 9000
cat "$scratch/lcov" "$scratch/lcov" >"$scratch/bad-lcov"
assert_exit 1 bash "$ratchet" "$scratch/bad-lcov" "$scratch/baseline" jeryu-api
for epsilon in 0.1 NaN -1; do
  assert_exit 1 env JERYU_COVERAGE_EPSILON="$epsilon" bash "$ratchet" "$scratch/lcov" "$scratch/baseline" jeryu-api
done
printf 'unrelated\t0.99\n' >>"$scratch/baseline"
assert_exit 0 env JERYU_COVERAGE_UPDATE_BASELINE=1 bash "$ratchet" "$scratch/lcov" "$scratch/baseline" jeryu-api
[[ $(cat "$scratch/baseline") == $'jeryu-api\t0.9000\nunrelated\t0.99' ]]
lcov 7000
assert_exit 0 env JERYU_COVERAGE_UPDATE_BASELINE=1 bash "$ratchet" "$scratch/lcov" "$scratch/baseline" jeryu-api
[[ $(cat "$scratch/baseline") == $'jeryu-api\t0.9000\nunrelated\t0.99' ]]
assert_exit 0 env JERYU_COVERAGE_UPDATE_BASELINE=1 bash "$ratchet" "$scratch/lcov" "$scratch/new-baseline" jeryu-api
[[ $(cat "$scratch/new-baseline") == $'jeryu-api\t0.7000' ]]
before=$(sha256sum "$scratch/baseline")
assert_exit 1 env JERYU_COVERAGE_UPDATE_BASELINE=1 bash "$ratchet" "$scratch/bad-lcov" "$scratch/baseline" jeryu-api
[[ $(sha256sum "$scratch/baseline") == "$before" ]]

# A PATH-local synthetic cargo produces only private test artifacts. It is never
# installed, exported from this process, or used to qualify the actual source.
mkdir "$scratch/bin"
cat >"$scratch/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
set -euo pipefail
[[ $1 == mutants ]] || exit 99
shift
if [[ ${1:-} == --version ]]; then
  printf '%s\n' "${FIXTURE_VERSION:-cargo-mutants 25.3.1}"
  exit "${FIXTURE_VERSION_RC:-0}"
fi
while [[ $# -gt 0 ]]; do
  if [[ $1 == --output ]]; then destination=$2; shift 2; else shift; fi
done
if [[ ${FIXTURE_NO_OUTPUT:-0} != 1 ]]; then
  mkdir "$destination/mutants.out"
  cp "$FIXTURE_JSON" "$destination/mutants.out/outcomes.json"
  cp "$FIXTURE_SELECTED" "$destination/mutants.out/mutants.json"
  printf '{"cargo_mutants_version":"25.3.1"}\n' >"$destination/mutants.out/lock.json"
fi
exit "${FIXTURE_RC:-0}"
CARGO
chmod 700 "$scratch/bin/cargo"
export PATH="$scratch/bin:$PATH"
export FIXTURE_JSON="$scratch/lab.json" FIXTURE_SELECTED="$scratch/selected.json"
export FIXTURE_RC=0 FIXTURE_NO_OUTPUT=0
jq -n '[{package:"synthetic-coverage-review",file:"crates/fixture/src/lib.rs",function:null,
  span:{start:{line:1,column:1},end:{line:1,column:2}},replacement:"false",genre:"BinaryOperator"}]' >"$FIXTURE_SELECTED"
jq -n --slurpfile mutants "$FIXTURE_SELECTED" '
  def phase($phase;$status): {phase:$phase,duration:0.1,process_status:$status,argv:["clearly-test-only"]};
  {total_mutants:1,missed:0,caught:1,timeout:0,unviable:0,success:0,outcomes:[
    {scenario:"Baseline",summary:"Success",phase_results:[phase("Build";"Success"),phase("Test";"Success")]},
    {scenario:{Mutant:$mutants[0][0]},summary:"CaughtMutant",phase_results:[phase("Build";"Success"),phase("Test";{Failure:101})]}]}
' >"$scratch/complete.json"
cp "$scratch/complete.json" "$FIXTURE_JSON"
run_mutation() { bash "$mutation" "$scratch/mutation-output" synthetic-coverage-review 60 300 1; }
assert_exit 0 run_mutation
cmp "$FIXTURE_JSON" "$scratch/mutation-output/mutants.out/outcomes.json"
cmp "$FIXTURE_JSON" "$scratch/mutation-output/outcomes.json"
for rc in 1 3 4 5 6 70 125 130 137; do
  export FIXTURE_RC=$rc
  assert_exit 1 run_mutation
done
export FIXTURE_RC=0 FIXTURE_NO_OUTPUT=1
before=$(sha256sum "$scratch/mutation-output/outcomes.json")
assert_exit 1 run_mutation
[[ $(sha256sum "$scratch/mutation-output/outcomes.json") == "$before" ]]
export FIXTURE_NO_OUTPUT=0
for edit in '.caught=2' '.outcomes=.outcomes[0:1]' '.outcomes += [.outcomes[1]]' '.outcomes[0].summary="Failure"' '.outcomes[0].phase_results[1].process_status={Failure:101}' '.outcomes[1].phase_results[1].process_status="Success"' '.outcomes[1].scenario.Mutant.package="wrong-package"' '.total_mutants=0' '.caught="1"' '.outcomes[1].phase_results[1].process_status={Signalled:9}'; do
  jq "$edit" "$scratch/complete.json" >"$FIXTURE_JSON"
  assert_exit 1 run_mutation
done
cat "$scratch/complete.json" "$scratch/complete.json" >"$FIXTURE_JSON"
assert_exit 1 run_mutation
jq '.missed=1 | .caught=0 | .outcomes[1].summary="MissedMutant" | .outcomes[1].phase_results[1].process_status="Success"' "$scratch/complete.json" >"$FIXTURE_JSON"
assert_exit 1 run_mutation
export FIXTURE_RC=2
assert_exit 0 run_mutation
cp "$scratch/complete.json" "$FIXTURE_JSON"
assert_exit 1 run_mutation
export FIXTURE_RC=0
# A complete run may contain unbuildable mutants, but it must execute tests on
# at least one viable mutant. The selected list, terminal outcomes, and totals
# still have to account for each distinct mutation exactly once.
cp "$FIXTURE_SELECTED" "$scratch/one-selected.json"
jq '. + [.[0] | .replacement="true"]' "$scratch/one-selected.json" >"$FIXTURE_SELECTED"
jq --slurpfile selected "$FIXTURE_SELECTED" '
  .total_mutants=2 | .unviable=1 | .outcomes += [{scenario:{Mutant:$selected[0][1]},
    summary:"Unviable",phase_results:[{phase:"Build",duration:0.1,process_status:{Failure:101},argv:["clearly-test-only"]}]}]
' "$scratch/complete.json" >"$FIXTURE_JSON"
assert_exit 0 run_mutation
cp "$scratch/complete.json" "$FIXTURE_JSON"
assert_exit 1 run_mutation
jq '. + .' "$scratch/one-selected.json" >"$FIXTURE_SELECTED"
assert_exit 1 run_mutation
printf '[]\n' >"$FIXTURE_SELECTED"
assert_exit 1 run_mutation
cp "$scratch/one-selected.json" "$FIXTURE_SELECTED"
jq '.caught=0 | .unviable=1 | .outcomes[1].summary="Unviable" |
  .outcomes[1].phase_results=[{phase:"Build",duration:0.1,process_status:{Failure:101},argv:["clearly-test-only"]}]' \
  "$scratch/complete.json" >"$FIXTURE_JSON"
assert_exit 1 run_mutation
export FIXTURE_VERSION='cargo-mutants 99.0.0'
assert_exit 1 run_mutation
export FIXTURE_VERSION='cargo-mutants 25.3.1' FIXTURE_VERSION_RC=1
assert_exit 1 run_mutation
printf '%s synthetic coverage evidence cases passed\n' "$cases"
