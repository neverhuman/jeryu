#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/ops/ci/common.sh"

cargo test -p jeryu-runner-core --jobs "${JERYU_CI_JOBS}" fscheck
cargo test -p jeryu-runner-native --jobs "${JERYU_CI_JOBS}" guards
cargo test -p jeryu-runner-oci --jobs "${JERYU_CI_JOBS}" oci_spec
cargo test -p jeryu-runnerd --jobs "${JERYU_CI_JOBS}" phase4_gates
cargo run -q -p jeryu-runnerd --jobs "${JERYU_CI_JOBS}" -- explain examples/jobs/denied-native-hot-fork.job >/tmp/jeryu-denied.json || status=$?
status="${status:-0}"
if [[ "$status" != "3" ]]; then
  echo "expected denied-native-hot-fork to exit 3, got $status" >&2
  exit 1
fi
echo "runner-sandbox static guard matrix: PASS"
echo "runner-sandbox live escape matrix: PENDING - native seccomp/Landlock/cgroups runtime not yet wired"
