#!/usr/bin/env python3
import json
from pathlib import Path

for path in Path("tests/fixtures").rglob("*.json"):
    if path.name.startswith("._") or any(part.startswith("._") for part in path.parts):
        continue
    json.loads(path.read_text(encoding="utf-8"))
print("fixtures ok")
