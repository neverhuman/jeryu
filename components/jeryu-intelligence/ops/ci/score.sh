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
score = report["score"]
if type(score) is not int or not 0 <= score <= 100:
    raise SystemExit("audit score must be an integer from 0 to 100")
# Policy errors must stop this gate; the maintained minimum remains 82.
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
floor = tomllib.loads(Path("agent/audit-policy.toml").read_text())["minimum_score"]
if type(floor) is not int or not 82 <= floor <= 100:
    raise SystemExit("audit policy minimum_score must be an integer from 82 to 100")
caps = report["caps_applied"]
findings = report["findings"]
decision = report["decision"]
if not isinstance(caps, list) or not isinstance(findings, list) or not isinstance(decision, dict):
    raise SystemExit("audit caps, findings or decision have an invalid shape")
if "caps" in report:
    if not isinstance(report["caps"], list):
        raise SystemExit("audit caps have an invalid shape")
    caps = caps + report["caps"]
hard_count = 0
for finding in findings:
    if not isinstance(finding, dict) or finding.get("severity") not in ("critical", "high", "medium", "low", "info"):
        raise SystemExit("audit finding has an invalid severity")
    if finding["severity"] in ("critical", "high") or finding.get("hardness") == "hard":
        hard_count += 1
# Recount actual findings even when an advisory decision reports zero hard findings.
for reported_hard in (report.get("hard_findings", 0), decision.get("hard_findings", 0)):
    if type(reported_hard) is not int or reported_hard < 0:
        raise SystemExit("audit hard-finding count must be a nonnegative integer")
    hard_count = max(hard_count, reported_hard)
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
