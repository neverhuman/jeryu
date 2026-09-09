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
  cargo check --locked --offline --manifest-path "$member_manifest" "${check_scope[@]}" \
    --all-targets --jobs "${JERYU_CI_JOBS:-40}"
fi
# End Cargo ownership check.

for script in scripts/*.sh ops/ci/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
printf 'check ok: %s\n' "$(pwd)"
