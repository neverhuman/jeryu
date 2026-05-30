#!/usr/bin/env bash
set -euo pipefail
cargo metadata --format-version 1 --no-deps >/dev/null
cargo fmt --all -- --check
cargo check -p jeryu-core -p jeryu-api -p jeryu-ci-ir -p jeryu-ci-compiler -p jeryu-ci-scheduler -p jeryu-proof -p jeryu-runner-protocol -p jeryu-runnerd --all-targets
cargo test -p jeryu-core -p jeryu-api -p jeryu-ci-compiler -p jeryu-ci-scheduler -p jeryu-proof -p jeryu-runner-protocol -p jeryu-runnerd
cargo test -p jeryu-cache-core -p jeryu-cache-service -p jeryu-cache-adversary
cargo test -p jeryu-cache-cli
./scripts/zero-evidence-guard.py .
