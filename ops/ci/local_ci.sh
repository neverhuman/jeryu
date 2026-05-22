#!/usr/bin/env bash
# ops/ci/local_ci.sh — deprecated wrapper for the old local CI matrix.
#
# Keep this as a thin compatibility shim only. The canonical full parity gate
# is scripts/ci-parity.sh and lane-level local dispatch lives in
# scripts/ci-local.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
printf '%s\n' "warning: ops/ci/local_ci.sh is deprecated; use scripts/ci-parity.sh or scripts/ci-local.sh" >&2
exec bash "$ROOT/scripts/ci-parity.sh" "$@"
