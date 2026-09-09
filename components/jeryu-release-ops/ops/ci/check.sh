#!/usr/bin/env bash
set -euo pipefail

source ops/ci/lib.sh
if [[ -f Cargo.toml ]]; then
  cargo metadata --format-version 1 --no-deps >/dev/null
  if [[ "${JERYU_SPLIT_FULL_CHECK:-0}" == "1" ]]; then
    cargo check --workspace --all-targets --jobs "${JERYU_CI_JOBS:-40}"
  fi
fi

if [[ -f package.json ]]; then
  node -e 'JSON.parse(require("fs").readFileSync("package.json", "utf8"))' >/dev/null
  if [[ -f apps/web/package.json ]]; then
    node -e 'JSON.parse(require("fs").readFileSync("apps/web/package.json", "utf8"))' >/dev/null
  fi
  if [[ "${JERYU_SPLIT_FULL_CHECK:-0}" == "1" ]]; then
    npm --workspace @jeryu/web run typecheck
  fi
fi

if [[ -f repos.manifest.toml ]]; then
  bash ops/ci/check-manifest.sh
fi
if [[ -d schemas ]]; then
  python3 - <<'PY'
import json
from pathlib import Path

for path in sorted(Path("schemas").glob("*.schema.json")):
    schema = json.loads(path.read_text())
    if schema.get("$schema") != "https://json-schema.org/draft/2020-12/schema":
        raise SystemExit(f"{path}: unsupported or missing JSON Schema dialect")
    if schema.get("type") != "object" or schema.get("additionalProperties") is not False:
        raise SystemExit(f"{path}: receipt schemas must be closed objects")
    properties = set(schema.get("properties", {}))
    required = set(schema.get("required", []))
    if not properties or properties != required:
        raise SystemExit(f"{path}: every receipt property must be required")
PY
fi
for script in scripts/*.sh ops/ci/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
bash ops/ci/test-governed-jankurai-path.sh
bash ops/ci/test-ci-local-dispatch.sh
printf 'check ok: %s\n' "$(pwd)"
