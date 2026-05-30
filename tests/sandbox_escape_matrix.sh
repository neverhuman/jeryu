#!/usr/bin/env bash
set -euo pipefail

cargo test -p runner-core fscheck
cargo test -p runner-native guards
cargo test -p runnerd phase4_gates
cargo run -q -p runnerd -- explain examples/jobs/denied-native-hot-fork.job >/tmp/jitforge-denied.json || status=$?
status="${status:-0}"
if [[ "$status" != "3" ]]; then
  echo "expected denied-native-hot-fork to exit 3, got $status" >&2
  exit 1
fi
