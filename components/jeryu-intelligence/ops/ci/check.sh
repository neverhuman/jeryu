#!/usr/bin/env bash
set -euo pipefail

source ops/ci/lib.sh
bash ops/ci/test-governed-jankurai-path.sh
# shellcheck source=ops/ci/cargo-scope.sh
source ops/ci/cargo-scope.sh
cargo test --locked --manifest-path "$member_manifest" --package jeryu-codegraph \
  --all-targets --jobs "${JERYU_CI_JOBS:-2}"
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
