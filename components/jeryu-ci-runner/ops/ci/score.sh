#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
require_jankurai

required=(
  agent/owner-map.json
  agent/test-map.json
  agent/generated-zones.toml
  agent/proof-lanes.toml
  agent/audit-policy.toml
  agent/boundaries.toml
  agent/JANKURAI_STANDARD.md
)
for path in "${required[@]}"; do
  [[ -s "$path" ]] || { printf 'missing split metadata: %s\n' "$path" >&2; exit 1; }
done
mkdir -p .jankurai target/jankurai
floor="$(audit_effective_floor agent/audit-policy.toml)"
jankurai audit . --full --mode advisory --policy agent/audit-policy.toml --fail-under "${floor}" --json .jankurai/repo-score.json --md .jankurai/repo-score.md
require_tool jq
# Validate the findings themselves: advisory summaries may report zero hard findings.
jq -es '
  length == 1 and (.[0] |
    type == "object"
    and (.score | type == "number" and . == floor and . >= 0 and . <= 100)
    and .caps_applied == []
    and (if has("caps") then .caps == [] else true end)
    and (.findings | type == "array" and all(.[];
      type == "object"
      and (.severity == "medium" or .severity == "low" or .severity == "info")
      and (if has("hardness") then .hardness == "soft" else true end)))
    and (if has("hard_findings") then .hard_findings == 0 else true end)
    and (.decision | type == "object"
      and (.minimum_score | type == "number" and . == floor and . >= 0 and . <= 100)
      and (if has("hard_findings") then .hard_findings == 0 else true end)))
' .jankurai/repo-score.json >/dev/null || {
  printf 'score check failed: malformed report, caps, or hard findings\n' >&2
  exit 1
}
score="$(jq -er '.score | select(type == "number") | floor' \
  .jankurai/repo-score.json)"
reported_floor="$(jq -er '.decision.minimum_score | select(type == "number") | floor' \
  .jankurai/repo-score.json)"
caps_count="$(jq -er '(.caps_applied // .caps // []) | if type == "array" then length else error("caps must be an array") end' .jankurai/repo-score.json)"
hard_count="$(jq -er '(.decision.hard_findings // .hard_findings // 0) | if type == "array" then length elif type == "number" then . else error("hard findings must be a number or array") end' .jankurai/repo-score.json)"
if (( score < floor || reported_floor != floor || caps_count != 0 ||
      hard_count != 0 )); then
  printf 'score check failed: score=%s floor=%s reported_floor=%s caps=%s hard_findings=%s\n' \
    "${score}" "${floor}" "${reported_floor}" "${caps_count}" \
    "${hard_count}" >&2
  exit 1
fi
cp .jankurai/repo-score.json target/jankurai/repo-score.json
cp .jankurai/repo-score.md target/jankurai/repo-score.md
printf 'score ok\n'
