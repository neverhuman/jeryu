#!/usr/bin/env bash
# Validate the immutable SPA bundle staged from jeryu-web and prove that the
# Deploy-owned API serves it. This repository intentionally carries no web
# source or npm workspace; build/typecheck/UX QA belong to jeryu-web's required
# gate before `scripts/stage-web-dist.sh` updates this vendored bundle.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"
source "${ROOT}/ops/ci/common.sh"

DIST="apps/web/dist"
INDEX="${DIST}/index.html"

if [ -e package.json ] || [ -e apps/web/package.json ]; then
  echo "web gate: unexpected npm source manifest in Deploy; jeryu-web owns buildable source" >&2
  exit 1
fi
if [ ! -s "${INDEX}" ]; then
  echo "web gate: missing or empty vendored SPA entrypoint: ${INDEX}" >&2
  exit 1
fi

asset_count=0
while IFS= read -r -d '' record; do
  mode="${record%% *}"
  path="${record#*$'\t'}"
  if [ "${mode}" != "100644" ]; then
    echo "web gate: vendored asset must be a regular non-executable Git blob: ${path} (${mode})" >&2
    exit 1
  fi
  if [ ! -f "${path}" ] || [ -L "${path}" ] || [ ! -s "${path}" ]; then
    echo "web gate: vendored asset is missing, empty, or a symlink: ${path}" >&2
    exit 1
  fi
  asset_count=$((asset_count + 1))
done < <(git ls-files -s -z -- "${DIST}")

if [ "${asset_count}" -lt 2 ]; then
  echo "web gate: vendored SPA bundle is incomplete (${asset_count} tracked file(s))" >&2
  exit 1
fi
if [ -n "$(git ls-files --others --exclude-standard -- "${DIST}")" ]; then
  echo "web gate: untracked files are present in the vendored SPA bundle" >&2
  git ls-files --others --exclude-standard -- "${DIST}" >&2
  exit 1
fi

reference_count=0
while IFS= read -r reference; do
  relative="${reference#/}"
  if [ -z "${relative}" ] || [[ "${relative}" == *'?'* ]] ||
     [[ "${relative}" == *'#'* ]] || [[ "${relative}" == *\\* ]] ||
     [[ "/${relative}/" == *'/../'* ]] || [[ "/${relative}/" == *'/./'* ]] ||
     [[ "${relative}" == //* ]]; then
    echo "web gate: index contains an unsafe local asset reference: ${reference}" >&2
    exit 1
  fi
  if [ ! -s "${DIST}/${relative}" ] || [ -L "${DIST}/${relative}" ]; then
    echo "web gate: index references a missing, empty, or symlinked asset: ${reference}" >&2
    exit 1
  fi
  if ! git ls-files --error-unmatch -- "${DIST}/${relative}" >/dev/null 2>&1; then
    echo "web gate: index references an untracked asset: ${reference}" >&2
    exit 1
  fi
  reference_count=$((reference_count + 1))
done < <(grep -oE '(src|href)="/[^"]+"' "${INDEX}" | sed -E 's/^[^=]+="([^"]+)"$/\1/' | sort -u)

if [ "${reference_count}" -eq 0 ]; then
  echo "web gate: vendored SPA entrypoint contains no local asset references" >&2
  exit 1
fi

echo "web gate: ${asset_count} immutable assets and ${reference_count} index references are intact"
cargo test --locked -p jeryu-api --features web --jobs "${JERYU_CI_JOBS}" \
  browser_repo_routes_serve_the_spa_shell
echo "web gate: Deploy API SPA integration passed"
