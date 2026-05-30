#!/usr/bin/env bash
# Shared local CI defaults. Keep this file source-only.
set -euo pipefail

export JERYU_CI_JOBS="${JERYU_CI_JOBS:-40}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-${JERYU_CI_JOBS}}"
