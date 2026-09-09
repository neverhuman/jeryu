#!/usr/bin/env bash
set -euo pipefail

source ops/ci/lib.sh
# shellcheck source=ops/ci/cargo-scope.sh
source ops/ci/cargo-scope.sh
if [[ "${JERYU_SPLIT_FULL_CHECK:-0}" == "1" ]]; then
  check_scope=(--workspace)
  if [[ $component_root != "$git_root" ]]; then
    check_scope=()
    for package in "${owned_packages[@]}"; do check_scope+=(--package "$package"); done
  fi
  cargo check --locked --manifest-path "$member_manifest" "${check_scope[@]}" \
    --all-targets --jobs "${JERYU_CI_JOBS:-40}"
fi
# End Cargo ownership check.

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
