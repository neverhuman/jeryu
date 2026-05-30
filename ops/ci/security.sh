#!/usr/bin/env bash
set -euo pipefail
cargo test -p jeryu-cache-adversary
./tests/cache_poisoning_matrix.sh
