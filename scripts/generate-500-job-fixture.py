#!/usr/bin/env python3
from pathlib import Path
root = Path(__file__).resolve().parents[1]
out = root / "tests/fixtures/github/500_jobs.yml"
lines = ["name: bench-500", "on: [push]", "jobs:"]
for i in range(500):
    name = f"job_{i:03d}"
    lines.append(f"  {name}:")
    lines.append("    runs-on: native-rust-clean")
    if i:
        lines.append(f"    needs: [job_{i-1:03d}]")
    lines.append("    steps:")
    lines.append(f"      - name: check {i}")
    lines.append(f"        run: echo job {i}")
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text("\n".join(lines) + "\n")
print(out)
