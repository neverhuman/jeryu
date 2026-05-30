#!/usr/bin/env bash
set -euo pipefail
cargo test -p cratevault-adversary
./tests/cache_poisoning_matrix.sh
