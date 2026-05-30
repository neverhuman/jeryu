#!/usr/bin/env bash
set -euo pipefail
cargo test -p jitforge-obs object_store_latency
