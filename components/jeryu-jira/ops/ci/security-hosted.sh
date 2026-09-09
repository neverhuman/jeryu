#!/usr/bin/env bash
# Thin hosted-workflow adapter for the canonical strict security lane.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
wrapper="${root}/tools/security-lane.sh"
system_bash="/usr/bin/bash"

if [[ ! -x "${system_bash}" || -L "${system_bash}" ||
      ! -f "${wrapper}" || -L "${wrapper}" ]]; then
  printf 'hosted security delegation custody failed\n' >&2
  exit 1
fi

exec "${system_bash}" "${wrapper}" "$@"
