#!/usr/bin/env bash
set -euo pipefail

source ops/ci/lib.sh
if [[ -f Cargo.toml ]]; then
  cargo metadata --locked --offline --format-version 1 --no-deps >/dev/null
  if [[ "${JERYU_SPLIT_FULL_CHECK:-0}" == "1" ]]; then
    cargo check --locked --offline --workspace --all-targets \
      --jobs "${JERYU_CI_JOBS:-40}"
  fi
fi

for script in scripts/*.sh ops/ci/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
printf 'check ok: %s\n' "$(pwd)"
