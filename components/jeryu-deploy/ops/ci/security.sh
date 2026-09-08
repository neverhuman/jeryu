#!/usr/bin/env bash
# Compatibility entrypoint; the repository-owned authority lives under tools/.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec bash "${ROOT}/tools/security-lane.sh" "$@"
