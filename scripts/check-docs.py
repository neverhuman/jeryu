#!/usr/bin/env python3
from pathlib import Path

required = {
    Path("README.md"): ["JitForge", "CrateVault", "Phase 12"],
    Path("docs/engineering_spec.md"): ["JitForge Nitro Engineering Spec", "Cache correctness beats cache hit rate"],
    Path("docs/PHASE12_SPEC.md"): ["Phase 12", "Zero false hits"],
    Path("docs/RUNBOOKS.md"): ["runbooks"],
}

missing = []
for path, needles in required.items():
    if not path.exists():
        missing.append(f"missing required doc: {path}")
        continue
    text = path.read_text(encoding="utf-8")
    for needle in needles:
        if needle not in text:
            missing.append(f"{path} missing marker: {needle}")

if missing:
    for item in missing:
        print(item)
    raise SystemExit(1)

print("docs ok")
