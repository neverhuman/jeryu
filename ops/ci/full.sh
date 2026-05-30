#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
cargo metadata --format-version 1 --no-deps >/dev/null
cargo fmt --all -- --check
cargo check --workspace --all-targets --jobs "${JERYU_CI_JOBS}"
cargo test --workspace --jobs "${JERYU_CI_JOBS}"
cargo clippy --workspace --all-targets --all-features --jobs "${JERYU_CI_JOBS}" -- -D warnings
jeryu_gate jeryu-evidence .
jeryu_gate jeryu-mapcheck docs
jeryu_gate jeryu-repogate release-gate
jeryu_gate jeryu-repogate score
./scripts/ci-doctor.sh
