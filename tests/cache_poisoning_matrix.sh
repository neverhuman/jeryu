#!/usr/bin/env bash
set -euo pipefail
cargo test -p cratevault-core policy
cargo test -p cratevault-core cache_key
cargo test -p cratevault-adversary adversarial_cache_laws_block_known_attacks
