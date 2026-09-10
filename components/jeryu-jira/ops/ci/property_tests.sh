#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh

cargo test --locked -p jeryu-jira --test work_properties --jobs "${JERYU_CI_JOBS:-40}"
printf 'property tests ok: %s\n' "$(pwd)"
