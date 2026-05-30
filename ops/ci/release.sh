#!/usr/bin/env bash
set -euo pipefail
cargo test --workspace
cargo build --workspace --release
./scripts/emit-release-receipt.sh
