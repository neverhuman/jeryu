#!/usr/bin/env bash
# Canonical strict security entrypoint for local and hosted lanes.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
security_script="${root}/ops/ci/security.sh"
system_bash="/usr/bin/bash"

if [[ ! -x "${system_bash}" || -L "${system_bash}" ]]; then
  printf 'canonical Bash is unavailable: %s\n' "${system_bash}" >&2
  exit 1
fi
if [[ ! -f "${security_script}" || -L "${security_script}" ]]; then
  printf 'security implementation must be a regular non-symlink file: %s\n' \
    "${security_script}" >&2
  exit 1
fi

export JERYU_REQUIRE_SECURITY_TOOLS=1
cd "${root}"
exec "${system_bash}" ops/ci/security.sh "$@"
