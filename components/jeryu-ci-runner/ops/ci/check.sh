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
if [[ -f Cargo.toml ]]; then
  cargo_metadata="$(cargo metadata --locked --format-version 1 --no-deps)"
  mapfile -t workspace_versions < <(
    jq -r '[.workspace_members[] as $member | .packages[] |
      select(.id == $member) | .version] | unique[]' <<< "${cargo_metadata}"
  )
  if [[ "${#workspace_versions[@]}" -ne 1 ||
        "${workspace_versions[0]}" != 5.0.0 ]]; then
    printf 'workspace package versions must all equal VERSION major/minor/patch 5.0.0\n' >&2
    exit 1
  fi
  if [[ "${JERYU_SPLIT_FULL_CHECK:-0}" == "1" ]]; then
    cargo check --locked --workspace --all-targets --jobs "${JERYU_CI_JOBS:-40}"
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

if [[ -e repos.manifest.toml || -L repos.manifest.toml ]]; then
  printf 'jeryu-ci-runner may not own an authority repos.manifest.toml\n' >&2
  exit 1
fi
for script in scripts/*.sh ops/ci/*.sh tools/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
printf 'check ok: %s\n' "$(pwd)"
