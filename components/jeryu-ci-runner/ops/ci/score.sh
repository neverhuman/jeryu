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
