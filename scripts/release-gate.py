#!/usr/bin/env python3
from pathlib import Path

required = [
    "Cargo.toml",
    "docs/engineering_spec.md",
    "docs/PHASE12_SPEC.md",
    "crates/cratevault-core/src/lib.rs",
    "crates/cratevault-service/src/lib.rs",
    "crates/runner-core/src/lib.rs",
    "crates/rustjet/src/lib.rs",
    "bins/jit-ci/src/main.rs",
]
missing = [path for path in required if not Path(path).exists()]
if missing:
    for path in missing:
        print(f"release gate missing {path}")
    raise SystemExit(1)
print("release gate ok")
