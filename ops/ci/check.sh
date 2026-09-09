#!/usr/bin/env bash
set -euo pipefail

source ops/ci/lib.sh
if [[ -f Cargo.toml ]]; then
  cargo metadata --locked --format-version 1 --no-deps >/dev/null
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

if [[ -f repos.manifest.toml ]]; then
  cargo run --locked --quiet -p jeryu-split-tool --bin jeryu-split -- \
    manifest --manifest repos.manifest.toml >/dev/null
fi
for script in scripts/*.sh ops/ci/*.sh tools/*.sh tests/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
bash tests/score-policy-hostiles.sh
bash tests/score-report-hostiles.sh
bash tests/scratch-hostiles.sh
bash tests/auxiliary-proofs.sh
bash tests/ci-matrix.sh
bash tests/component-ci-dispatch.sh
bash tests/clone-family-hostiles.sh
bash tests/security-lane-hostiles.sh
printf 'check ok: %s\n' "$(pwd)"
