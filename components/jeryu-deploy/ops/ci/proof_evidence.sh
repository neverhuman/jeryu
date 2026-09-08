#!/usr/bin/env bash
# Compatibility spelling. The hyphenated script is the only implementation.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "${HERE}/proof-evidence.sh" "$@"
