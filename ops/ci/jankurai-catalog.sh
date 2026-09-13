#!/usr/bin/env bash
# Compatibility entrypoint for complete predecessor and auxiliary proof admission.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
exec bash "$root/scripts/auxiliary-proofs.sh" "$@"
