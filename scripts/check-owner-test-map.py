#!/usr/bin/env python3
import json
from pathlib import Path

required_roots = [
    "agent/**",
    "bins/**",
    "config/**",
    "configs/**",
    "crates/**",
    "docs/**",
    "examples/**",
    "fixtures/**",
    "ops/**",
    "policies/**",
    "scripts/**",
    "tests/**",
]

owner = json.loads(Path("agent/owner-map.json").read_text(encoding="utf-8"))
tests = json.loads(Path("agent/test-map.json").read_text(encoding="utf-8"))

owner_patterns = {pattern for entry in owner.get("owners", []) for pattern in entry.get("paths", [])}
test_patterns = {pattern for entry in tests.get("routes", []) for pattern in entry.get("paths", [])}

missing_owner = sorted(set(required_roots) - owner_patterns)
missing_tests = sorted(set(required_roots) - test_patterns)

bad_owners = [entry for entry in owner.get("owners", []) if not entry.get("paths") or not entry.get("owners")]
bad_tests = [entry for entry in tests.get("routes", []) if not entry.get("paths") or not entry.get("commands") or not entry.get("proof_lane")]

if missing_owner or missing_tests or bad_owners or bad_tests:
    if missing_owner:
        print("missing owner paths:", missing_owner)
    if missing_tests:
        print("missing test paths:", missing_tests)
    if bad_owners:
        print("bad owner entries:", bad_owners)
    if bad_tests:
        print("bad test entries:", bad_tests)
    raise SystemExit(1)

print("owner/test map ok")
