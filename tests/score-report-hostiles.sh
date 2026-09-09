#!/usr/bin/env bash
# Exercise report admission and the owning Cache/Runner shell score policies.
# Python policies are tested separately by score-policy-hostiles.sh.
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
require_tool() { command -v "$1" >/dev/null; }
run_governed_jankurai() { printf 'governed-transport\n' >>calls; jankurai "$@"; }
jankurai() {
  [[ $1 == audit ]] || return 97
  if (( SCORE_TEST_SHELL_POLICY )); then
    [[ " $* " == *" --fail-under $SCORE_TEST_EXPECTED_FLOOR "* ]] || return 96
  fi
  printf 'audit\n' >>calls
  (( SCORE_TEST_AUDITOR_STATUS == 0 )) || return "$SCORE_TEST_AUDITOR_STATUS"
  cat report-input.json >.jankurai/repo-score.json
  printf 'synthetic report\n' >.jankurai/repo-score.md
}
# Stop Python transport here; its actual policy code has a separate hostile suite.
python3() { printf 'policy-transport-not-executed\n' >>calls; return 79; }
LIB
# Test Runner's actual floor helper without its unrelated hosted tool setup.
sed -n '/^readonly JERYU_FLEET_MINIMUM_SCORE=/p;/^audit_effective_floor() {$/,/^}$/p' \
  "$root/components/jeryu-ci-runner/ops/ci/lib.sh" >>"$scratch/ops/ci/lib.sh"
printf 'printf "adoption\\n" >>calls\n' >"$scratch/ops/ci/tool-adoption.sh"
template='{"score":85,"caps_applied":[],"findings":[],"decision":{"minimum_score":85,"hard_findings":0,"passed":true,"status":"advisory"}}'
cases=0
expect() {
  local expected=$1 report=$2 auditor_status=${3:-0} audit_expected=${4:-1} actual
  printf '%s\n' "$report" >"$scratch/report-input.json"
  : >"$scratch/calls"
  if (cd "$scratch" && SCORE_TEST_AUDITOR_STATUS=$auditor_status \
      SCORE_TEST_SHELL_POLICY=$shell_policy SCORE_TEST_EXPECTED_FLOOR=$policy_floor bash "$script") \
    >"$scratch/stdout" 2>"$scratch/stderr"; then
    actual=0
  else
    actual=$?
  fi
  [[ $actual == "$expected" ]] || {
    printf '%s case %s: expected exit %s, got %s\n' "$component" "$cases" "$expected" "$actual" >&2
    return 1
  }
  local expected_calls='' artifact
  if [[ $component == jeryu-tool ]]; then expected_calls=$'adoption\n'; fi
  if [[ $component == jeryu-deploy ]]; then expected_calls=$'governed-transport\n'; fi
  if (( audit_expected )); then expected_calls+=audit; fi
  if (( expected == 79 )); then expected_calls+=$'\npolicy-transport-not-executed'; fi
  [[ $(<"$scratch/calls") == "$expected_calls" ]] || {
    printf '%s case %s: unexpected producer/policy order\n' "$component" "$cases" >&2
    return 1
  }
  if (( expected == 0 )); then
    cmp "$scratch/report-input.json" "$scratch/target/jankurai/repo-score.json"
    cmp "$scratch/.jankurai/repo-score.md" "$scratch/target/jankurai/repo-score.md"
    for artifact in "$scratch/target/jankurai/repo-score.json" "$scratch/target/jankurai/repo-score.md"; do
      [[ -f $artifact && ! -L $artifact && -O $artifact && $(stat -c '%h' -- "$artifact") == 1 ]]
      rm -- "$artifact"
    done
  else
    [[ ! -e "$scratch/target/jankurai/repo-score.json" &&
       ! -e "$scratch/target/jankurai/repo-score.md" ]]
  fi
  cases=$((cases + 1))
}
for component in jeryu-intelligence jeryu-release-ops jeryu-tool jeryu-web \
  jeryu-cache jeryu-ci-runner jeryu-deploy jeryu-jira; do
  script="$root/components/$component/ops/ci/score.sh"
  shell_policy=0 policy_floor=85 success=79
  printf 'minimum_score = 85\n' >"$scratch/agent/audit-policy.toml"
  case $component in
    jeryu-cache) shell_policy=1 success=0 ;;
    jeryu-ci-runner)
      shell_policy=1 policy_floor=91 success=0
      printf 'minimum_score = 80\n' >"$scratch/agent/audit-policy.toml"
      ;;
  esac
  valid=$(jq --argjson floor "$policy_floor" '.score=$floor|.decision.minimum_score=$floor' <<<"$template")
  expect "$success" "$valid"
  expect "$success" "$(jq '.findings=[{severity:"medium",hardness:"soft"},{severity:"low"},{severity:"info"}]' <<<"$valid")"
  if (( shell_policy )); then empty_decision=1; else empty_decision=79; fi
  expect "$empty_decision" "$(jq '.hard_findings=0|.caps=[]|.decision={}' <<<"$valid")"
  for score in 0 64 65 81 82 84 85 90 91 92 100; do
    expected=$success
    if (( shell_policy && score < policy_floor )); then expected=1; fi
    expect "$expected" "$(jq --argjson score "$score" '.score=$score' <<<"$valid")"
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
  if (( shell_policy )); then
    for mutation in '.decision.minimum_score=85.5' '.decision.minimum_score="85"' \
      '.decision.minimum_score=true' '.decision.minimum_score=-1' \
      '.decision.minimum_score=101' 'del(.decision.minimum_score)' '.decision.minimum_score=0'; do
      expect 1 "$(jq "$mutation" <<<"$valid")"
    done
    # These validate the unique floor token, not arbitrary TOML syntax; the
    # governed auditor remains responsible for parsing the complete policy.
    for policy in 'minimum_score = true' 'minimum_score = "85"' \
      'minimum_score = 85.5' 'minimum_score = -1' 'minimum_score = 101' \
      'minimum_score = 18446744073709551616' 'unknown = 85' \
      $'minimum_score = 85\nminimum_score = 90'; do
      printf '%s\n' "$policy" >"$scratch/agent/audit-policy.toml"
      expect 1 "$valid" 0 0
    done
    if [[ $component == jeryu-cache ]]; then
      printf 'minimum_score = 84\n' >"$scratch/agent/audit-policy.toml"
      expect 1 "$valid" 0 0
    else
      for configured in 0 80 90; do
        printf 'minimum_score = %s\n' "$configured" >"$scratch/agent/audit-policy.toml"
        expect 0 "$valid"
      done
    fi
    for policy_floor in 92 100; do
      printf 'minimum_score = %s\n' "$policy_floor" >"$scratch/agent/audit-policy.toml"
      raised=$(jq --argjson floor "$policy_floor" '.score=$floor|.decision.minimum_score=$floor' <<<"$valid")
      expect 0 "$raised"
      expect 1 "$(jq '.score-=1' <<<"$raised")"
    done
  fi
done
printf '%s report/producer cases passed, including Cache/Runner shell policies; Python policy transport was not executed\n' "$cases"
