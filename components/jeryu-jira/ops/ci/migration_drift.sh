#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh

cargo test --locked -p jeryu-jira --test sqlite_migrations --jobs "${JERYU_CI_JOBS:-40}"
printf 'migration drift ok: %s\n' "$(pwd)"
