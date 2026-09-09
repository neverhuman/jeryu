#!/usr/bin/env bash
# Compatibility entrypoint for the canonical standalone proof-evidence lane.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "${HERE}/proof-evidence.sh" "$@"
