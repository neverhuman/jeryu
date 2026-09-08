#!/usr/bin/env bash
set -euo pipefail
cargo test -p jeryu-obs db_failover
