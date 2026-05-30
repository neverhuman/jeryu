#!/usr/bin/env python3
import json
import sys
from pathlib import Path

score = 100
advisories = []
required = [
    "AGENTS.md",
    "Justfile",
    "rust-toolchain.toml",
    "Cargo.toml",
    "agent/owner-map.json",
    "agent/test-map.json",
    "agent/proof-lanes.toml",
    "agent/generated-zones.toml",
    "agent/baselines/main.repo-score.json",
    "docs/engineering_spec.md",
    "docs/PHASE12_SPEC.md",
    "policies/cache-laws.toml",
]
for raw in required:
    if not Path(raw).exists():
        score -= 8
        advisories.append(f"missing {raw}")

workspace = Path("Cargo.toml").read_text(encoding="utf-8") if Path("Cargo.toml").exists() else ""
for member in ["crates/jeryu-cache-core", "crates/jeryu-cache-service", "crates/jeryu-runner-core", "crates/jeryu-rustjet", "crates/jeryu-gitd"]:
    if member not in workspace:
        score -= 3
        advisories.append(f"workspace missing {member}")

result = {
    "repo": "jeryu",
    "phase": 12,
    "score": max(score, 0),
    "required_exit_score": 95,
    "hard_blocks": [] if score >= 95 else advisories,
    "advisories": advisories,
}
if "--write-baseline" in sys.argv:
    Path("agent/baselines/main.repo-score.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print(json.dumps(result, indent=2))
if result["score"] < result["required_exit_score"]:
    raise SystemExit(1)
