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
  python3 - <<'PY'
from pathlib import Path
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
data = tomllib.loads(Path("repos.manifest.toml").read_text())
repos = data.get("repo", [])
if not repos:
    raise SystemExit("repos.manifest.toml has no [[repo]] entries")
if "jeryu" not in data.get("required_repos", []):
    raise SystemExit("repos.manifest.toml must require the public portal repo")
PY
fi
for script in scripts/*.sh ops/ci/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
printf 'check ok: %s\n' "$(pwd)"
