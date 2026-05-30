#!/usr/bin/env bash
set -euo pipefail

cargo test -p jeryu-runner-core fscheck
cargo test -p jeryu-runner-native guards
cargo test -p jeryu-runnerd phase4_gates
cargo run -q -p jeryu-runnerd -- explain examples/jobs/denied-native-hot-fork.job >/tmp/jeryu-denied.json || status=$?
status="${status:-0}"
if [[ "$status" != "3" ]]; then
  echo "expected denied-native-hot-fork to exit 3, got $status" >&2
  exit 1
fi
