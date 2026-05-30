#!/usr/bin/env python3
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
owner = json.loads((ROOT / "agent" / "owner-map.json").read_text(encoding="utf-8"))
tests = json.loads((ROOT / "agent" / "test-map.json").read_text(encoding="utf-8"))
required_roots = {"agent", "bins", "config", "configs", "crates", "docs", "examples", "fixtures", "ops", "policies", "scripts", "tests"}

def roots_from_patterns(entries):
    roots = set()
    for entry in entries:
        for pattern in entry.get("paths", []):
            roots.add(pattern.split("/", 1)[0])
    return roots

owner_roots = roots_from_patterns(owner.get("owners", []))
test_roots = roots_from_patterns(tests.get("routes", []))
missing_owner = sorted(required_roots - owner_roots)
missing_tests = sorted(required_roots - test_roots)

if missing_owner or missing_tests:
    raise SystemExit(f"missing owner={missing_owner} tests={missing_tests}")

for entry in owner.get("owners", []):
    if not entry.get("paths") or not entry.get("owners") or entry.get("required_reviews", 0) < 1:
        raise SystemExit(f"bad owner map entry: {entry}")
for entry in tests.get("routes", []):
    if not entry.get("paths") or not entry.get("commands") or not entry.get("proof_lane"):
        raise SystemExit(f"bad test map entry: {entry}")

print("agent maps cover repository paths")
