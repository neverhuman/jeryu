#!/usr/bin/env bash
# Execute owning score scripts with the real Rust gate; auditor transport is synthetic.
# Cache/Runner retain their shell policies. No Python interpreter is executed.
set -euo pipefail
mode=all
case $# in
  0) ;;
  1)
    case $1 in
      --root-only) mode=root ;;
      --policy-matrix) mode=policy ;;
      *) printf 'usage: %s [--root-only|--policy-matrix]\n' "$0" >&2; exit 2 ;;
    esac
    ;;
  *) printf 'usage: %s [--root-only|--policy-matrix]\n' "$0" >&2; exit 2 ;;
esac
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
# Bind the root gate transport to the actual binary built from this workspace.
(cd "$root" && cargo build --locked --offline --quiet -p jeryu-split-tool \
  --bin jeryu-split --message-format=json) >"$scratch/score-gate-build.json"
SCORE_TEST_GATE_BIN=$(jq -ers '
  [.[] | select(.reason == "compiler-artifact"
    and .target.name == "jeryu-split" and .executable != null)]
  | if length == 1 then .[0].executable
    else error("expected one owning score-gate executable") end
' "$scratch/score-gate-build.json")
[[ $SCORE_TEST_GATE_BIN == /* && -x $SCORE_TEST_GATE_BIN ]]
export SCORE_TEST_GATE_BIN
mkdir "$scratch/agent" "$scratch/ops" "$scratch/ops/ci" "$scratch/schemas" "$scratch/scripts"
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
# Root Cargo invocation is only a transport seam: execute the real built gate.
cargo() {
  local expected=(run --locked --offline --quiet -p jeryu-split-tool --bin jeryu-split
    -- audit-score-check --owner jeryu --policy agent/audit-policy.toml
    --report .jankurai/repo-score.json)
  local index=0 argument
  [[ ${SCORE_TEST_RUST_POLICY:-0} == 1 && $# == ${#expected[@]} ]] || return 98
  for argument in "$@"; do
    [[ $argument == "${expected[$index]}" ]] || return 98
    index=$((index + 1))
  done
  printf 'rust-policy\n' >>calls
  shift 9
  "$SCORE_TEST_GATE_BIN" "$@"
}
# Only the owning script's closed Git root lookup is synthetic.
env() {
  local expected=(-i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null
    GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_NO_REPLACE_OBJECTS=1
    GIT_OPTIONAL_LOCKS=0 /usr/bin/git -c core.fsmonitor=false rev-parse --show-toplevel)
  local index=0 argument
  [[ $# == ${#expected[@]} ]] || return 98
  for argument in "$@"; do
    [[ $argument == "${expected[$index]}" ]] || return 98
    index=$((index + 1))
  done
  printf '%s\n' "$PWD"
}
LIB
# Installation/root selection is covered separately by score-transport-hostiles.sh.
# This narrow transport checks the literal owner and runs the real gate on real inputs.
cat >"$scratch/scripts/check-audit-score.sh" <<'GATE'
set -euo pipefail
[[ $# == 4 && $1 == --owner && $2 == "$SCORE_TEST_OWNER" &&
   $3 == --component-root && $4 == "$PWD" ]] || exit 98
printf 'policy\n' >>calls
exec "$SCORE_TEST_GATE_BIN" audit-score-check --owner "$2" \
  --policy "$PWD/agent/audit-policy.toml" --report "$PWD/.jankurai/repo-score.json"
GATE
# Test Runner's actual floor helper without its unrelated hosted tool setup.
sed -n '/^readonly JERYU_FLEET_MINIMUM_SCORE=/p;/^audit_effective_floor() {$/,/^}$/p' \
  "$root/components/jeryu-ci-runner/ops/ci/lib.sh" >>"$scratch/ops/ci/lib.sh"
printf 'printf "adoption\\n" >>calls\n' >"$scratch/ops/ci/tool-adoption.sh"
template='{"score":85,"caps_applied":[],"findings":[],"decision":{"minimum_score":85,"hard_findings":0,"passed":true,"status":"advisory"}}'
cases=0
expect() {
  local expected=$1 report=$2 auditor_status=${3:-0} audit_expected=${4:-1} actual
  local policy_expected=${5:-1}
  printf '%s\n' "$report" >"$scratch/report-input.json"
  : >"$scratch/calls"
  if (cd "$scratch" && SCORE_TEST_AUDITOR_STATUS=$auditor_status \
      SCORE_TEST_SHELL_POLICY=$shell_policy SCORE_TEST_RUST_POLICY=$rust_policy \
      SCORE_TEST_OWNER=$component SCORE_TEST_EXPECTED_FLOOR=$policy_floor bash "$script") \
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
  if (( rust_policy && audit_expected && auditor_status == 0 )); then
    expected_calls+=$'\nrust-policy'
  elif (( ! shell_policy && audit_expected && auditor_status == 0 )) &&
       [[ $component == jeryu-core || $policy_expected == 1 ]]; then
    expected_calls+=$'\npolicy'
  fi
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
# The exact 32 original policy fixtures per owner now traverse actual scripts.
if [[ $mode == policy ]]; then
  for component in jeryu jeryu-core jeryu-deploy jeryu-jira jeryu-intelligence \
    jeryu-release-ops jeryu-tool jeryu-web; do
    script="$root/components/$component/ops/ci/score.sh"
    shell_policy=0 rust_policy=0 policy_floor=85 minimum=85
    case $component in
      jeryu) script="$root/ops/ci/score.sh"; rust_policy=1 ;;
      jeryu-intelligence) minimum=82 ;;
      jeryu-tool) minimum=65 ;;
    esac
    for specification in '85 85 0' '85 84 1' '90 89 1' '90 90 0' '100 100 0'; do
      read -r floor score expected <<<"$specification"
      printf 'minimum_score = %s\n' "$floor" >"$scratch/agent/audit-policy.toml"
      expect "$expected" "{\"score\":$score,\"caps_applied\":[],\"findings\":[],\"decision\":{}}"
    done
    printf 'minimum_score = 85\n' >"$scratch/agent/audit-policy.toml"
    for fields in \
      '{"score":100,"caps_applied":["cap"]}' \
      '{"score":100,"caps":["cap"]}' \
      '{"score":100,"hard_findings":1}' \
      '{"score":100,"decision":{"hard_findings":1}}' \
      '{"score":100,"decision":{"hard_findings":0},"hard_findings":1}' \
      '{"score":true}' '{"score":"100"}' '{"score":100.0}' '{"score":101}' \
      '{"score":100,"caps_applied":{}}' \
      '{"score":100,"caps_applied":[],"caps":["concealed-cap"]}' \
      '{"score":100,"findings":{}}' \
      '{"score":100,"findings":[{"severity":"high"}]}' \
      '{"score":100,"findings":[{"severity":"critical"}]}' \
      '{"score":100,"findings":[{"severity":"low","hardness":"hard"}]}' \
      '{"score":100,"findings":[{"severity":"unknown"}]}' \
      '{"score":100,"decision":{"hard_findings":-1}}' \
      '{"score":100,"decision":{"hard_findings":true}}'; do
      report=$(jq -c --argjson fields "$fields" \
        '. + $fields' <<<'{"caps_applied":[],"findings":[],"decision":{}}')
      policy_expected=0
      if [[ $fields == '{"score":100.0}' ]]; then
        # Preserve the original floating token; jq normalizes it to an integer.
        report='{"score":100.0,"caps_applied":[],"findings":[],"decision":{}}'
        policy_expected=1
      fi
      expect 1 "$report" 0 1 "$policy_expected"
    done
    for policy in '' 'minimum_score = [' 'minimum_score = true' \
      'minimum_score = "85"' 'minimum_score = 85.0' 'minimum_score = 101'; do
      printf '%s\n' "$policy" >"$scratch/agent/audit-policy.toml"
      expect 1 '{"score":100,"caps_applied":[],"findings":[],"decision":{}}'
    done
    printf 'minimum_score = %s\n' "$((minimum - 1))" >"$scratch/agent/audit-policy.toml"
    expect 1 '{"score":100,"caps_applied":[],"findings":[],"decision":{}}'
    printf 'minimum_score = %s\n' "$minimum" >"$scratch/agent/audit-policy.toml"
    expect 0 "{\"score\":$minimum,\"caps_applied\":[],\"findings\":[],\"decision\":{}}"
    expect 1 "{\"score\":$((minimum - 1)),\"caps_applied\":[],\"findings\":[],\"decision\":{}}"
  done
  [[ $cases == 256 ]] || { printf 'required policy case count changed: %s\n' "$cases" >&2; exit 1; }
  printf '%s owning-script Rust policy/report cases passed; report suite not run\n' "$cases"
  exit 0
fi

components=(jeryu jeryu-intelligence jeryu-release-ops jeryu-tool jeryu-web
  jeryu-cache jeryu-ci-runner jeryu-deploy jeryu-jira)
if [[ $mode == root ]]; then components=(jeryu); fi
for component in "${components[@]}"; do
  script="$root/components/$component/ops/ci/score.sh"
  shell_policy=0 rust_policy=0 policy_floor=85 success=0
  printf 'minimum_score = 85\n' >"$scratch/agent/audit-policy.toml"
  case $component in
    jeryu) script="$root/ops/ci/score.sh"; rust_policy=1 success=0 ;;
    jeryu-cache) shell_policy=1 success=0 ;;
    jeryu-ci-runner)
      shell_policy=1 policy_floor=91 success=0
      printf 'minimum_score = 80\n' >"$scratch/agent/audit-policy.toml"
      ;;
  esac
  valid=$(jq --argjson floor "$policy_floor" '.score=$floor|.decision.minimum_score=$floor' <<<"$template")
  expect "$success" "$valid"
  expect "$success" "$(jq '.findings=[{severity:"medium",hardness:"soft"},{severity:"low"},{severity:"info"}]' <<<"$valid")"
  if (( shell_policy )); then empty_decision=1; else empty_decision=$success; fi
  expect "$empty_decision" "$(jq '.hard_findings=0|.caps=[]|.decision={}' <<<"$valid")"
  for score in 0 64 65 81 82 84 85 90 91 92 100; do
    expected=$success
    if (( score < policy_floor )); then expected=1; fi
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
    expect 1 "$(jq "$mutation" <<<"$valid")" 0 1 0
  done
  expect 1 "$valid
$valid" 0 1 0
  expect 1 '' 0 1 0
  expect 1 '{' 0 1 0
  expect 1 '[]' 0 1 0
  expect 1 'null' 0 1 0
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

if [[ $mode == root ]]; then
  [[ $cases == 59 ]] || { printf 'required root report case count changed: %s\n' "$cases" >&2; exit 1; }
  printf '%s root report/producer cases passed; scoped Rust root gate only, full suite not run\n' "$cases"
  exit 0
fi

# Exercise the actual Deploy PR audit-through-security dispatch. The earlier
# build/web stages are outside this report-admission regression. Only auditor
# transport and unrelated security/lock work are synthetic; the owning score
# entrypoint, retained jq preflight and real Rust policy gate execute.
pr_script="$root/components/jeryu-deploy/ops/ci/pr-ci.sh"
printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail' \
  'source ops/ci/lib.sh' \
  'jeryu_deploy_assert_workspace_lock_unchanged() { printf "lock\\n" >>calls; }' \
  >"$scratch/pr-audit.sh"
sed -n '/^echo "\[pr-ci\] jankurai audit /,$p' "$pr_script" >>"$scratch/pr-audit.sh"
[[ $(rg -c '^echo "\[pr-ci\] jankurai audit ' "$scratch/pr-audit.sh") == 1 ]]
cat >"$scratch/ops/ci/score.sh" <<'SCORE'
printf 'score\n' >>calls
exec bash "$SCORE_TEST_DEPLOY_SCORE"
SCORE
cat >"$scratch/ops/ci/security.sh" <<'SECURITY'
[[ ${JERYU_SECURITY_NETWORK:-} == 1 ]] || exit 95
printf 'security\n' >>calls
exit "$SCORE_TEST_SECURITY_STATUS"
SECURITY
expect_pr() {
  local expected=$1 report=$2 expected_calls=$3 policy=${4:-85}
  local auditor_status=${5:-0} security_status=${6:-0} actual
  printf 'minimum_score = %s\n' "$policy" >"$scratch/agent/audit-policy.toml"
  printf '%s\n' "$report" >"$scratch/report-input.json"
  : >"$scratch/calls"
  if (cd "$scratch" && repo_root="$scratch" SCORE_TEST_OWNER=jeryu-deploy \
      SCORE_TEST_DEPLOY_SCORE="$root/components/jeryu-deploy/ops/ci/score.sh" \
      SCORE_TEST_SHELL_POLICY=0 SCORE_TEST_EXPECTED_FLOOR=85 \
      SCORE_TEST_AUDITOR_STATUS=$auditor_status SCORE_TEST_SECURITY_STATUS=$security_status \
      bash "$scratch/pr-audit.sh") >"$scratch/stdout" 2>"$scratch/stderr"; then
    actual=0
  else
    actual=$?
  fi
  [[ $actual == "$expected" && $(<"$scratch/calls") == "$expected_calls" ]] || {
    printf 'Deploy PR score case %s: unexpected exit/dispatch (wanted %s, got %s)\n' \
      "$cases" "$expected" "$actual" >&2
    return 1
  }
  if (( expected == 0 )); then
    rg -q '^\[pr-ci\] PASS ' "$scratch/stderr"
  elif rg -q '^\[pr-ci\] PASS ' "$scratch/stderr"; then
    printf 'Deploy PR emitted PASS after a failed required command\n' >&2
    return 1
  fi
  cases=$((cases + 1))
}
pr_audit=$'score\ngoverned-transport\naudit'
pr_policy="$pr_audit"$'\npolicy'
pr_success="$pr_policy"$'\nsecurity\nlock'
expect_pr 0 "$template" "$pr_success"
for mutation in '.findings=[{severity:"high",hardness:"hard"}]' \
  '.findings=[{severity:"critical",hardness:"hard"}]' \
  '.findings=[{severity:"low",hardness:"hard"}]' '.caps=["hidden cap"]' \
  '.score="85"' 'del(.findings)'; do
  expect_pr 1 "$(jq "$mutation" <<<"$template")" "$pr_audit"
done
expect_pr 1 "$template" "$pr_policy" 90
expect_pr 1 "$template" "$pr_policy" true
expect_pr 1 "$(jq '.score=84' <<<"$template")" "$pr_policy"
expect_pr 23 "$template" "$pr_audit" 85 23
expect_pr 29 "$template" "$pr_policy"$'\nsecurity' 85 0 29
expect_pr 0 "$(jq '.score=90|.decision.minimum_score=90' <<<"$template")" "$pr_success" 90
[[ $cases == 586 ]] || { printf 'required report case count changed: %s\n' "$cases" >&2; exit 1; }
printf '%s report/producer and Deploy PR dispatch cases passed; real Rust gates and existing Cache/Runner shell policies executed\n' "$cases"
