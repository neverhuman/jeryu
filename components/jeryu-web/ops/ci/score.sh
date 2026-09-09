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
python3 - <<'PY'
import json
import sys
from pathlib import Path
report = json.loads(Path(".jankurai/repo-score.json").read_text())
score = int(report.get("score") or 0)
# Per-repo floor comes from the audit policy (default 85): profile-calibrated,
# e.g. the public-portal scaffold has no product code for several categories.
floor = 85
try:
    import tomllib
    floor = int(tomllib.loads(Path("agent/audit-policy.toml").read_text()).get("minimum_score", 85))
except Exception:
    pass
caps = report.get("caps_applied") or report.get("caps") or []
decision = report.get("decision") if isinstance(report.get("decision"), dict) else {}
hard = decision.get("hard_findings", report.get("hard_findings", 0))
if isinstance(hard, list):
    hard_count = len(hard)
else:
    hard_count = int(hard or 0)
errors = []
if score < floor:
    errors.append(f"score {score} is below {floor}")
if caps:
    errors.append(f"caps present: {', '.join(str(item) for item in caps)}")
if hard_count:
    errors.append(f"hard findings present: {hard_count}")
if errors:
    print("score check failed: " + "; ".join(errors), file=sys.stderr)
    sys.exit(1)
PY
cp .jankurai/repo-score.json target/jankurai/repo-score.json
cp .jankurai/repo-score.md target/jankurai/repo-score.md
printf 'score ok\n'
