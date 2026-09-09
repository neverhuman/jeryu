#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

PATTERN='(AKIA[0-9A-Z]{16}|-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----|gh[pousr]_[A-Za-z0-9_]{36,}|xox[baprs]-[A-Za-z0-9-]{20,}|AIza[0-9A-Za-z_-]{35}|[A-Za-z0-9_]*SECRET[A-Za-z0-9_]*[[:space:]]*=[[:space:]]*["'\''][^"'\'']{12,}["'\''])'

if rg -n --hidden --glob '!target/**' --glob '!.git/**' --glob '!.jankurai/**' --glob '!Cargo.lock' -e "${PATTERN}" .; then
  echo "secret-scan: potential secret material found" >&2
  exit 1
fi

echo "secret-scan: PASS"
