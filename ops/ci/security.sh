#!/usr/bin/env bash
# Compatibility entrypoint; the canonical implementation is tools/security-lane.sh.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec bash "${ROOT}/tools/security-lane.sh" "$@"
