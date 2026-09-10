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
  cargo run --locked --offline --quiet -p jeryu-split-tool --bin jeryu-split -- \
    manifest --manifest repos.manifest.toml >/dev/null
fi
for script in scripts/*.sh ops/ci/*.sh ops/deploy/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
bash scripts/check-agent-maps.sh
bash scripts/test-ci-phases.sh
bash scripts/test-coverage-evidence.sh
bash scripts/test-workspace-lock.sh
bash scripts/test-web-build.sh
printf 'check ok: %s\n' "$(pwd)"
