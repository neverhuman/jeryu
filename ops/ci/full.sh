#!/usr/bin/env bash
set -euo pipefail
cargo metadata --format-version 1 --no-deps >/dev/null
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
./scripts/zero-evidence-guard.py .
./scripts/check-docs.py
./scripts/release-gate.py
./scripts/score-repo.py
./scripts/ci-doctor.sh
