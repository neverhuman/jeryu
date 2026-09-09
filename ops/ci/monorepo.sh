#!/usr/bin/env bash
# Hosted CI delegates to the same command implementation used locally.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
exec bash "$root/scripts/ci.sh" "$@"
