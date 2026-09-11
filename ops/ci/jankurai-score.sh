#!/usr/bin/env bash
# Local and hosted audits use the complete maintained census.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
[[ $# == 0 ]] || { printf 'usage: ops/ci/jankurai-score.sh\n' >&2; exit 2; }
exec bash "$root/scripts/ci.sh" audit
