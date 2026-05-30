#!/usr/bin/env python3
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover for old Python only
    import tomli as tomllib

path = Path("agent/generated-zones.toml")
config = tomllib.loads(path.read_text(encoding="utf-8"))
zones = {zone.get("path"): zone for zone in config.get("zones", [])}
required = {
    "docs/generated/**": "scripts/render-policy-docs.sh",
    "receipts/generated/**": "jeryu-cache-service",
}

missing = []
for zone_path, generator in required.items():
    zone = zones.get(zone_path)
    if zone is None:
        missing.append(f"missing generated zone: {zone_path}")
    elif zone.get("generator") != generator:
        missing.append(f"generated zone {zone_path} has generator {zone.get('generator')!r}, expected {generator!r}")
    elif zone.get("manual_edits") is not False:
        missing.append(f"generated zone {zone_path} must set manual_edits = false")

if missing:
    for item in missing:
        print(item)
    raise SystemExit(1)

print("generated zones ok")
