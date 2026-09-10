#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
require_jankurai
require_tool jq

required=(
  agent/owner-map.json
  agent/test-map.json
  agent/generated-zones.toml
  agent/proof-lanes.toml
  agent/audit-policy.toml
  agent/boundaries.toml
  agent/JANKURAI_STANDARD.md
  agent/tool-adoption.toml
  agent/coverage-sources.toml
  schemas/repair-queue.schema.json
  schemas/repair-receipt.schema.json
)
for path in "${required[@]}"; do
  [[ -s "$path" ]] || { printf 'missing quality metadata: %s\n' "$path" >&2; exit 1; }
done
mkdir -p .jankurai target/jankurai
bash ops/ci/tool-adoption.sh
jankurai audit . --full --mode advisory --policy agent/audit-policy.toml --json .jankurai/repo-score.json --md .jankurai/repo-score.md
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
      and (if has("hard_findings") then .hard_findings == 0 else true end)))
' .jankurai/repo-score.json >/dev/null || {
  printf 'score check failed: malformed report, caps, or hard findings\n' >&2
  exit 1
}
score_root=$(env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
  GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_NO_REPLACE_OBJECTS=1 \
  GIT_OPTIONAL_LOCKS=0 /usr/bin/git -c core.fsmonitor=false rev-parse --show-toplevel)
bash "$score_root/scripts/check-audit-score.sh" --owner jeryu-tool \
  --component-root "$(pwd -P)" "$@" >/dev/null
cp .jankurai/repo-score.json target/jankurai/repo-score.json
cp .jankurai/repo-score.md target/jankurai/repo-score.md
printf 'score ok\n'
