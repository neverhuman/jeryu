#!/usr/bin/env bash
set -euo pipefail

source ops/ci/lib.sh
require_tool jq
expected_release_identity='jeryu-ci-runner-v5.0.0-split.1'
if [[ ! -f VERSION || -L VERSION || "$(stat -c '%h' -- VERSION)" != 1 ||
      "$(wc -l < VERSION)" != 1 || "$(<VERSION)" != "${expected_release_identity}" ||
      "$(tail -c 1 VERSION | od -An -tx1 | tr -d '[:space:]')" != 0a ]]; then
  printf 'VERSION must be the exact one-link release identity %s\n' \
    "${expected_release_identity}" >&2
  exit 1
fi
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

if [[ -e repos.manifest.toml || -L repos.manifest.toml ]]; then
  printf 'jeryu-ci-runner may not own an authority repos.manifest.toml\n' >&2
  exit 1
fi
for script in scripts/*.sh ops/ci/*.sh tools/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
printf 'check ok: %s\n' "$(pwd)"
