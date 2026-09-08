#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$repo_root"

if ! command -v npm >/dev/null 2>&1; then
  printf 'contract-drift: npm is required\n' >&2
  exit 1
fi

for path in package.json package-lock.json apps/web/package.json apps/web/package-lock.json; do
  if [[ ! -f "$path" || -L "$path" ]]; then
    printf 'contract-drift: required input is missing or not a physical file: %s\n' "$path" >&2
    exit 1
  fi
done

if [[ ! -d apps/web/node_modules ]]; then
  npm ci --prefix apps/web
fi

npm --workspace @jeryu/web run test:contracts
printf 'contract-drift ok\n'
