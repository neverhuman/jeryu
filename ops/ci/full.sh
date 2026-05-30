#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
cargo metadata --format-version 1 --no-deps >/dev/null
cargo fmt --all -- --check
cargo check --workspace --all-targets --jobs "${JERYU_CI_JOBS}"
cargo test --workspace --jobs "${JERYU_CI_JOBS}"
cargo clippy --workspace --all-targets --all-features --jobs "${JERYU_CI_JOBS}" -- -D warnings
./scripts/zero-evidence-guard.py .
./scripts/check-docs.py
./scripts/release-gate.py
./scripts/score-repo.py
./scripts/ci-doctor.sh
