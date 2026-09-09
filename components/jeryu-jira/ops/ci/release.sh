#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh

mkdir -p target/jankurai

[[ -s VERSION ]] || { printf 'release readiness failed: VERSION is missing\n' >&2; exit 1; }
[[ -s CHANGELOG.md ]] || { printf 'release readiness failed: CHANGELOG.md is missing\n' >&2; exit 1; }
[[ -s docs/release.md ]] || { printf 'release readiness failed: docs/release.md is missing\n' >&2; exit 1; }
[[ -s docs/rollback.md ]] || { printf 'release readiness failed: docs/rollback.md is missing\n' >&2; exit 1; }
[[ -s target/jankurai/security/evidence.json || -s target/security/evidence.json ]] || {
  printf 'release readiness failed: run just security before release evidence\n' >&2
  exit 1
}

python3 - <<'PY'
import json
from pathlib import Path

version = Path("VERSION").read_text().strip()
Path("target/jankurai/release-readiness.json").write_text(json.dumps({
    "schema_version": "jeryu.split.release-readiness/v1",
    "version_source": "VERSION",
    "version": version,
    "changelog": "CHANGELOG.md",
    "release_process": "docs/release.md",
    "rollback": "docs/rollback.md",
    "security_evidence": "target/jankurai/security/evidence.json",
    "backup_policy": "SQLite backup required before Work schema-changing release",
    "monitoring_evidence": "target/jankurai/release-readiness.json",
    "abuse_controls": [
        "Rust request validation",
        "SQLite constraints",
        "structured bridge repair evidence",
    ],
    "artifact_support": "target/artifact-support/jeryu-jira.json",
}, indent=2, sort_keys=True) + "\n")
PY

printf 'release readiness ok: target/jankurai/release-readiness.json\n'
