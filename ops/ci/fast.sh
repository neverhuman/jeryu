#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
cargo metadata --format-version 1 --no-deps >/dev/null
cargo fmt --all -- --check
cargo check -p jeryu-core -p jeryu-api -p jeryu-ci-ir -p jeryu-ci-compiler -p jeryu-ci-scheduler -p jeryu-proof -p jeryu-runner-protocol -p jeryu-runnerd --all-targets --jobs "${JERYU_CI_JOBS}"
cargo test -p jeryu-core -p jeryu-api -p jeryu-ci-compiler -p jeryu-ci-scheduler -p jeryu-proof -p jeryu-runner-protocol -p jeryu-runnerd --jobs "${JERYU_CI_JOBS}"
cargo test -p jeryu-cache-core -p jeryu-cache-service -p jeryu-cache-adversary --jobs "${JERYU_CI_JOBS}"
cargo test -p jeryu-cache-cli --jobs "${JERYU_CI_JOBS}"
./scripts/zero-evidence-guard.py .
