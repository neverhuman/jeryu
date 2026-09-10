#!/usr/bin/env bash
# Pure report predicates; these values never enter score or release evidence paths.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
predicate="$root/ops/ci/score-report.jq"
valid='{"score":85,"raw_score":85,"caps_applied":[],"findings":[],
"decision":{"hard_findings":0,"passed":true,"status":"pass","minimum_score":85},
"scope":{"mode":"full","paths":[]},
"policy":{"mode":"standard","minimum_score":85,"fail_on":["critical","high"]},
"copy_code":{"status":"pass","classes":[],"summary":{"hard_classes":0,"hard_instances":0}}}'
cases=0
expect() {
  local expected=$1 minimum=$2 report=$3 actual=fail
  if jq -es --argjson minimum "$minimum" -f "$predicate" <<<"$report" >/dev/null 2>&1; then
    actual=pass
  fi
  [[ $actual == "$expected" ]] || {
    printf 'score report case %s expected %s, got %s\n' "$cases" "$expected" "$actual" >&2
    exit 1
  }
  cases=$((cases + 1))
}
expect pass 85 "$valid"
expect pass 85 "$(jq '.findings=[{severity:"medium",hardness:"soft"},{severity:"low",hardness:"soft"},{severity:"info",hardness:"soft"}]' <<<"$valid")"
expect pass 90 "$(jq '.score=90|.raw_score=90|.policy.minimum_score=90|.decision.minimum_score=90' <<<"$valid")"
expect pass 85 "$(jq '.copy_code.status="review"|.copy_code.classes=[{hard_fail:false,effective_severity:"warning"}]' <<<"$valid")"
for mutation in \
  '.score=84|.raw_score=84' '.score=101|.raw_score=101' \
  '.score=85.5|.raw_score=85.5' '.score=true|.raw_score=true' '.score="85"|.raw_score="85"' \
  '.raw_score=90' 'del(.score)' 'del(.caps_applied)' '.caps_applied={}' \
  '.caps_applied=["cap"]' '.caps=["concealed cap"]' '.caps=null' \
  'del(.findings)' '.findings={}' '.findings=[null]' \
  '.findings=[{severity:"high"}]' '.findings=[{severity:"critical"}]' \
  '.findings=[{severity:"unknown"}]' '.findings=[{severity:"low",hardness:"hard"}]' \
  '.findings=[{severity:"low"}]' '.findings=[{severity:"low",hardness:null}]' \
  '.findings=[{severity:"low",hardness:false}]' '.findings=[{severity:"low",hardness:[]}]' \
  '.findings=[{severity:"low",hardness:"unknown"}]' '.findings=[{severity:"low",hardness:"Hard"}]' \
  '.hard_findings=1' '.hard_findings="0"' '.hard_findings=false' \
  'del(.decision)' '.decision.hard_findings=1' '.decision.hard_findings=[]' \
  '.decision.passed=false' '.decision.status="advisory"' '.decision.minimum_score=65' \
  '.scope.mode="changed"' '.scope.paths=["src/main.rs"]' \
  '.policy.mode="advisory"' '.policy.minimum_score=65' '.policy.fail_on=[]' \
  '.copy_code.summary.hard_classes=1' '.copy_code.summary.hard_instances=1' \
  '.copy_code.status="skipped"' '.copy_code.status="fail"' 'del(.copy_code.status)' \
  '.copy_code.classes={}' 'del(.copy_code.classes)' '.copy_code.classes=[null]' \
  '.copy_code.classes=[{hard_fail:true,effective_severity:"hard"}]' \
  '.copy_code.classes=[{hard_fail:false,effective_severity:"unknown"}]' \
  '.copy_code.classes=[{effective_severity:"warning"}]' \
  'del(.copy_code)'; do
  expect fail 85 "$(jq "$mutation" <<<"$valid")"
done
for minimum in 84 101 true null '"85"' 85.5; do
  expect fail "$minimum" "$valid"
done
expect fail 90 "$valid"
expect fail 85 "$valid
$valid"
expect fail 85 ''
expect fail 85 '{'
printf '%s full-standard score report cases passed\n' "$cases"
