#!/usr/bin/env bash
# Compatibility entrypoint for the complete source/build/receipt acquisition.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
[[ $# == 0 ]] || { printf 'usage: ops/ci/install-jankurai-release.sh\n' >&2; exit 2; }
# Source the bootstrap to keep its verified executable descriptor alive.
source "$root/scripts/bootstrap-jankurai.sh"
bootstrap_public_jankurai
