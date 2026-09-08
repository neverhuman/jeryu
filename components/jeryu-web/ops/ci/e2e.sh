#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

mkdir -p target/jankurai/e2e

if [ ! -d apps/web/node_modules ]; then
  npm ci --prefix apps/web
fi

if ! find "${HOME}/.cache/ms-playwright" -maxdepth 1 -type d -name 'chromium-*' 2>/dev/null | grep -q .; then
  if [ -n "${CI:-}" ]; then
    npm --workspace @jeryu/web exec -- playwright install --with-deps chromium
  else
    npm --workspace @jeryu/web exec -- playwright install chromium
  fi
fi

npm --workspace @jeryu/web run test:e2e:actions
npm --workspace @jeryu/web run test:e2e:matrix

cat > target/jankurai/e2e/receipt.json <<'JSON'
{
  "schema_version": "jeryu.web.e2e/v1",
  "lane": "e2e",
  "mode": "ui-only",
  "project": "chromium",
  "action_matrix": "apps/web/e2e/action-matrix.json",
  "junit": "apps/web/playwright-report/junit.xml"
}
JSON
