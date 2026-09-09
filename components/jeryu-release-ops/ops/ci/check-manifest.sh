#!/usr/bin/env bash
# The monorepo candidate and the historical v5 authority have distinct schemas.
set -euo pipefail
component=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
root=$(env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
  /usr/bin/git -C "$component" rev-parse --show-toplevel)
if [[ $component == "$root/components/jeryu-release-ops" ]]; then
  cd "$root"
  cargo run --locked --quiet -p jeryu-split-tool --bin jeryu-split -- manifest --check-paths
else
  cd "$component"
  cargo run --locked --quiet -p jeryu-repogate -- family-manifest
fi
