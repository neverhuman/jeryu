#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

ci_log "running deterministic fast lane with ${JERYU_CI_JOBS} workers"
./ops/ci/fast.sh
ci_log "running security lane with ${JERYU_CI_JOBS} workers"
./ops/ci/security.sh
ci_log "running Jankurai audit lane"
./ops/ci/jankurai.sh
