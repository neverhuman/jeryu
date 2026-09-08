#!/usr/bin/env bash
set -euo pipefail
cargo test -p jeryu-obs object_store_latency
