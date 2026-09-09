#!/usr/bin/env bash
# Exercise owning report admission only. Policy parsing is deliberately not run.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
source "$root/tests/scratch.sh"
umask 077
scratch=$(mktemp -d "${TMPDIR:-/tmp}/jeryu-score-report.XXXXXXXX")
jeryu_record_test_scratch "$scratch"
finish() {
  local result=$?
  trap - EXIT
  if (( result == 0 )); then
    jeryu_remove_test_scratch || exit 1
  else
    printf 'score report test failed; retained fixture: %s\n' "$scratch" >&2
  fi
  exit "$result"
}
trap finish EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$scratch/agent" "$scratch/ops" "$scratch/ops/ci" "$scratch/schemas"
for path in owner-map.json test-map.json generated-zones.toml proof-lanes.toml \
  audit-policy.toml boundaries.toml JANKURAI_STANDARD.md tool-adoption.toml \
  coverage-sources.toml; do
  printf 'synthetic report-admission input\n' >"$scratch/agent/$path"
done
for path in repair-queue.schema.json repair-receipt.schema.json; do
  printf '{}\n' >"$scratch/schemas/$path"
done
cat >"$scratch/ops/ci/lib.sh" <<'LIB'
require_jankurai() { :; }
jankurai() {
  [[ $1 == audit ]] || return 97
  printf 'audit\n' >>calls
  (( SCORE_TEST_AUDITOR_STATUS == 0 )) || return "$SCORE_TEST_AUDITOR_STATUS"
  cat report-input.json >.jankurai/repo-score.json
  printf 'synthetic report\n' >.jankurai/repo-score.md
}
# Stop at the unchanged Python stage; never execute a Python interpreter.
python3() { printf 'policy-transport-not-executed\n' >>calls; return 79; }
LIB
printf 'printf "adoption\\n" >>calls\n' >"$scratch/ops/ci/tool-adoption.sh"
valid='{"score":85,"caps_applied":[],"findings":[],"decision":{"hard_findings":0,"passed":true,"status":"advisory"}}'
cases=0
expect() {
  local expected=$1 report=$2 auditor_status=${3:-0} actual
  printf '%s\n' "$report" >"$scratch/report-input.json"
  : >"$scratch/calls"
  if (cd "$scratch" && SCORE_TEST_AUDITOR_STATUS=$auditor_status bash "$script") \
    >"$scratch/stdout" 2>"$scratch/stderr"; then
    actual=0
  else
    actual=$?
  fi
  [[ $actual == "$expected" ]] || {
    printf '%s case %s: expected exit %s, got %s\n' "$component" "$cases" "$expected" "$actual" >&2
    return 1
  }
  local expected_calls=
  if [[ $component == jeryu-tool ]]; then expected_calls=$'adoption\n'; fi
  expected_calls+=audit
  if (( expected == 79 )); then expected_calls+=$'\npolicy-transport-not-executed'; fi
  [[ $(<"$scratch/calls") == "$expected_calls" ]] || {
    printf '%s case %s: unexpected producer/policy order\n' "$component" "$cases" >&2
    return 1
  }
  # Admission and stopped policy transport never publish target score artifacts.
  [[ ! -e "$scratch/target/jankurai/repo-score.json" &&
     ! -e "$scratch/target/jankurai/repo-score.md" ]]
  cases=$((cases + 1))
}
for component in jeryu-intelligence jeryu-release-ops jeryu-tool jeryu-web; do
  script="$root/components/$component/ops/ci/score.sh"
  expect 79 "$valid"
  expect 79 "$(jq '.findings=[{severity:"medium",hardness:"soft"},{severity:"low"},{severity:"info"}]' <<<"$valid")"
  expect 79 "$(jq '.hard_findings=0|.caps=[]|.decision={}' <<<"$valid")"
  # These are report shape/range checks only; they do not prove any policy floor.
  for score in 0 64 65 81 82 84 85 90 100; do
    expect 79 "$(jq --argjson score "$score" '.score=$score' <<<"$valid")"
  done
  for mutation in \
    '.score=-1' '.score=101' '.score=85.5' '.score=true' '.score="85"' \
    'del(.score)' '.caps_applied={}' '.caps_applied=null' 'del(.caps_applied)' \
    '.caps_applied=["cap"]' '.caps=["concealed cap"]' '.caps=null' '.caps={}' '.caps=false' \
    '.findings={}' '.findings=null' 'del(.findings)' '.findings=[null]' \
    '.findings=[{severity:"high"}]' '.findings=[{severity:"critical"}]' \
    '.findings=[{severity:"low",hardness:"hard"}]' '.findings=[{severity:"unknown"}]' \
    '.findings=[{}]' '.findings=[{severity:"low",hardness:null}]' \
    '.findings=[{severity:"low",hardness:false}]' '.findings=[{severity:"low",hardness:"unknown"}]' \
    '.hard_findings=1' '.hard_findings=-1' '.hard_findings="0"' '.hard_findings=false' '.hard_findings=[]' \
    '.decision.hard_findings=1' '.decision.hard_findings=-1' \
    '.decision.hard_findings="0"' '.decision.hard_findings=false' '.decision.hard_findings=[]' \
    '.decision=null' '.decision=[]' 'del(.decision)'; do
    expect 1 "$(jq "$mutation" <<<"$valid")"
  done
  expect 1 "$valid
$valid"
  expect 1 ''
  expect 1 '{'
  expect 1 '[]'
  expect 1 'null'
  expect 23 "$valid" 23
done
printf '%s owning report admission/producer cases passed; policy parsing was not executed\n' "$cases"
