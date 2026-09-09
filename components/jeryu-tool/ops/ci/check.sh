#!/usr/bin/env bash
# Structure check: typed manifests parse, the generated pin is current, and
# every Rust/shell entrypoint passes its locked offline gate.
set -euo pipefail
source ops/ci/lib.sh

bash ops/ci/check-rendered-identity.sh --check --repo jeryu-tool
bash ops/test-install-jankurai.sh
bash ops/test-public-candidate-transaction.sh
bash ops/test-bootstrap-jankurai-root-seal.sh
bash ops/test-render-tool-manifest.sh
bash ops/test-governed-jankurai-path.sh

# Reusable-tool registry and renderer are the same locked Rust control binary.
bash ops/registry-summary.sh --check
cargo fmt --all -- --check
cargo test --locked --offline --all-targets
cargo clippy --locked --offline --all-targets -- -D warnings
for script in ops/*.sh ops/ci/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
require_tool shellcheck
mapfile -d '' shell_scripts < <(git ls-files -z '*.sh')
for script in "${shell_scripts[@]}"; do
  [[ "$script" == ops/render-assets/require-jankurai.sh ]] && continue
  shellcheck --severity=warning --external-sources "$script"
done
jq empty agent/*.json schemas/*.json
bash ops/test-doctor-controls.sh
printf 'check ok: %s\n' "$(pwd)"
