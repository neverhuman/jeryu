#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh

if [[ -e crates/jeryu-jira/bindings ]]; then
  printf 'contract drift failed: generated TypeScript must live only in contracts/generated\n' >&2
  exit 1
fi

cargo test -p jeryu-jira --test contract_drift --jobs "${JERYU_CI_JOBS:-40}"
printf 'contract drift ok: %s\n' "$(pwd)"
