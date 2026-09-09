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
python3 - <<'PY'
import json
import sys
from pathlib import Path
report = json.loads(Path(".jankurai/repo-score.json").read_text())
score = int(report.get("score") or 0)
floor = 85
try:
    import tomllib
    floor = int(tomllib.loads(Path("agent/audit-policy.toml").read_text()).get("minimum_score", 85))
except Exception:
    pass
caps = report.get("caps_applied") or report.get("caps") or []
decision = report.get("decision") if isinstance(report.get("decision"), dict) else {}
hard = decision.get("hard_findings", report.get("hard_findings", 0))
hard_count = len(hard) if isinstance(hard, list) else int(hard or 0)
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
