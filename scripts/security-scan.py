#!/usr/bin/env python3
from pathlib import Path

blocked = ["unsafe {", "std::process::Command::new(\"sh\")"]
violations = []
for root in [Path("crates"), Path("bins")]:
    for path in root.rglob("*.rs"):
        if path.name.startswith("._") or any(part.startswith("._") for part in path.parts):
            continue
        text = path.read_text(encoding="utf-8")
        for needle in blocked:
            if needle in text:
                violations.append((str(path), needle))
if violations:
    for path, needle in violations:
        print(f"security violation {path}: {needle}")
    raise SystemExit(1)
print("security scan ok")
