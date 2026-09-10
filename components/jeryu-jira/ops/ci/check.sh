#!/usr/bin/env bash
set -euo pipefail

source ops/ci/lib.sh
# shellcheck source=ops/ci/cargo-scope.sh
source ops/ci/cargo-scope.sh

bash ops/ci/test-governed-jankurai-path.sh
cargo fmt --manifest-path "$member_manifest" --package "${owned_packages[@]}" --check
cargo check --locked --manifest-path "$member_manifest" --package "${owned_packages[@]}" --all-targets --jobs "${JERYU_CI_JOBS:-40}"
cargo test --locked --manifest-path "$member_manifest" --package "${owned_packages[@]}" --lib --bins --jobs "${JERYU_CI_JOBS:-40}"

for script in scripts/*.sh ops/ci/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done

printf 'check ok: %s\n' "$(pwd)"
