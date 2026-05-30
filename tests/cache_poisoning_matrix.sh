#!/usr/bin/env bash
set -euo pipefail
cargo test -p jeryu-cache-core policy
cargo test -p jeryu-cache-core cache_key
cargo test -p jeryu-cache-adversary adversarial_cache_laws_block_known_attacks
