#!/usr/bin/env bash
set -euo pipefail
cargo test -p jitforge-obs db_failover
