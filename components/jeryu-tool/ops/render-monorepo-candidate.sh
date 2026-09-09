#!/usr/bin/env bash
# Explicit unendorsed monorepo consumer rendering. No installed state is changed.
set -euo pipefail
here="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
tool_root="$(cd -P -- "${here}/.." && pwd)"
monorepo_root="$(cd -P -- "${tool_root}/../.." && pwd)"
args=("$@")
explicit_root=0
for argument in "${args[@]}"; do
  [[ "${argument}" != --monorepo-root ]] || explicit_root=1
done
if [[ "${explicit_root}" == 0 ]]; then
  args=(--monorepo-root "${monorepo_root}" "${args[@]}")
fi
exec cargo run --quiet --locked --offline \
  --manifest-path "${tool_root}/crates/jeryu-tool-control/Cargo.toml" --bin jeryu-toolctl -- \
  --tool-root "${tool_root}" render-monorepo-candidate \
  "${args[@]}"
