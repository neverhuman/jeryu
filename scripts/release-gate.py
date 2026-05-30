#!/usr/bin/env python3
from pathlib import Path

required = [
    "Cargo.toml",
    "docs/engineering_spec.md",
    "docs/PHASE12_SPEC.md",
    "crates/jeryu-cache-core/src/lib.rs",
    "crates/jeryu-cache-service/src/lib.rs",
    "crates/jeryu-runner-core/src/lib.rs",
    "crates/jeryu-rustjet/src/lib.rs",
    "bins/jeryu-ci-bin/src/main.rs",
]
missing = [path for path in required if not Path(path).exists()]
if missing:
    for path in missing:
        print(f"release gate missing {path}")
    raise SystemExit(1)
print("release gate ok")
