#!/usr/bin/env bash
set -euo pipefail
runs="${1:-100}"
for _ in $(seq 1 "$runs"); do
  cargo run -q -p jeryu-runnerd -- self-test
done
