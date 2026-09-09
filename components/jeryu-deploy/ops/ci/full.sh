#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=ops/ci/common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
cargo metadata --locked --format-version 1 --no-deps >/dev/null
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets --jobs "${JERYU_CI_JOBS}"
cargo test --locked --workspace --jobs "${JERYU_CI_JOBS}" -- \
  --test-threads "${JERYU_CI_TEST_THREADS}"
cargo clippy --locked --workspace --all-targets --all-features --jobs "${JERYU_CI_JOBS}" -- -D warnings
bash ops/ci/proof-evidence.sh
bash ops/ci/workflow-lint.sh
bash scripts/check-agent-maps.sh
bash scripts/test-emit-release-receipt.sh
bash ops/ci/score.sh
JERYU_SECURITY_NETWORK=1 bash ops/ci/security.sh
bash ops/ci/dependency-sources.sh
./scripts/ci-doctor.sh
