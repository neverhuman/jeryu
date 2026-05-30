#!/usr/bin/env bash
set -euo pipefail
required=(
  AGENTS.md
  agent/JANKURAI_STANDARD.md
  agent/owner-map.json
  agent/test-map.json
  agent/proof-lanes.toml
  agent/generated-zones.toml
  agent/standard-version.toml
  agent/baselines/main.repo-score.json
  ops/ci/fast.sh
  ops/ci/full.sh
  ops/ci/audit.sh
  ops/ci/security.sh
  ops/ci/release.sh
  scripts/ci-local.sh
  scripts/ci-doctor.sh
  Justfile
  rust-toolchain.toml
  config/jeryu-cache-policy.toml
  config/trust-tiers.toml
  policies/cache-laws.toml
)
for path in "${required[@]}"; do
  test -f "$path" || { echo "missing required file: $path" >&2; exit 1; }
done
for script in ops/ci/*.sh scripts/*.sh tests/*.sh; do
  [[ "$(basename "$script")" == ._* ]] && continue
  bash -n "$script"
done
python3 - <<'PYCHECK'
import json
from pathlib import Path
for raw in [
    'agent/owner-map.json',
    'agent/test-map.json',
    'agent/baselines/main.repo-score.json',
    'examples/cache-key-material.json',
    'examples/fork-pr-write-request.json',
    'examples/t1-green-write-request.json',
]:
    path = Path(raw)
    if path.name.startswith('._'):
        continue
    with path.open('r', encoding='utf-8') as fh:
        json.load(fh)
print('json fixtures ok')
PYCHECK
./scripts/check-docs.py
./scripts/check-generated-zones.py
./scripts/check-owner-test-map.py
./scripts/check-agent-maps.py
./scripts/check-fixtures.py
./scripts/security-scan.py
printf '%s\n' 'ci-doctor passed'
