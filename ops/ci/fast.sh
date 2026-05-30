#!/usr/bin/env bash
set -euo pipefail
cargo fmt --all -- --check
cargo test -p cratevault-core -p cratevault-service -p cratevault-adversary
cargo test -p cratevault-cli
