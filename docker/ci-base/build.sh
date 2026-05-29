#!/usr/bin/env bash
# Build, smoke-test, and (optionally) push the jeryu/ci-base CI runtime image.
#
# Required env (auto-detected in GitLab CI):
#   CI_REGISTRY               eg. registry.local-gitlab.example
#   CI_REGISTRY_USER          (push only)
#   CI_REGISTRY_PASSWORD      (push only)
#
# Optional env:
#   IMAGE_REPO    default: ${CI_REGISTRY:-jeryu}/jeryu/ci-base
#   PRIMARY_TAG   default: 1.95.0  (matches Dockerfile RUST_VERSION ARG)
#   EXTRA_TAGS    space-separated additional tags  (default: "latest")
#   PUSH          1 to docker push after build      (default: 0)
#   PLATFORMS     buildx target platforms           (default: linux/amd64)
#
# Run from repo root or anywhere:
#   bash docker/ci-base/build.sh
#
# Exit codes:
#   0  build + smoke ok (and push if PUSH=1)
#   1  docker missing or build failed
#   2  smoke failed
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

IMAGE_REPO="${IMAGE_REPO:-${CI_REGISTRY:-jeryu}/jeryu/ci-base}"
PRIMARY_TAG="${PRIMARY_TAG:-1.95.0}"
EXTRA_TAGS="${EXTRA_TAGS:-latest}"
PLATFORMS="${PLATFORMS:-linux/amd64}"
PUSH="${PUSH:-0}"

log() { printf '\033[1;34m[ci-base]\033[0m %s\n' "$*" >&2; }
die() { printf '\033[0;31m[ci-base/err]\033[0m %s\n' "$*" >&2; exit "${2:-1}"; }

command -v docker >/dev/null 2>&1 || die "docker not found in PATH" 1

# Build tag list
tag_args=( "-t" "${IMAGE_REPO}:${PRIMARY_TAG}" )
for t in $EXTRA_TAGS; do
  tag_args+=( "-t" "${IMAGE_REPO}:${t}" )
done

# Build flags: buildx if available (multi-platform support, modern features);
# else fall back to classic docker build (single platform only).
if docker buildx version >/dev/null 2>&1; then
  builder=buildx
  build_cmd=( docker buildx build --platform "${PLATFORMS}" --pull --progress=plain )
  if [ "${PUSH}" = "1" ]; then
    build_cmd+=( --push )
  else
    build_cmd+=( --load )
  fi
else
  builder=classic
  build_cmd=( docker build --pull --progress=plain )
fi

log "builder=${builder} image=${IMAGE_REPO}:${PRIMARY_TAG} extras=${EXTRA_TAGS} push=${PUSH}"
"${build_cmd[@]}" \
  --file "${SCRIPT_DIR}/Dockerfile" \
  "${tag_args[@]}" \
  "${SCRIPT_DIR}" \
  || die "docker build failed" 1

# Push path: only meaningful for classic builder (buildx --push above)
if [ "${builder}" = "classic" ] && [ "${PUSH}" = "1" ]; then
  log "pushing tags"
  docker push "${IMAGE_REPO}:${PRIMARY_TAG}"
  for t in $EXTRA_TAGS; do
    docker push "${IMAGE_REPO}:${t}"
  done
fi

# Smoke test (always — catches missing tools / version drift before push too,
# even though the Dockerfile's final RUN already smokes. This re-runs from a
# fresh container, exercising the actual runtime environment).
log "smoke test: container starts and tools resolve at expected pins"
docker run --rm "${IMAGE_REPO}:${PRIMARY_TAG}" bash -lc '
  set -euo pipefail
  rustc --version          | grep -F "1.95.0"
  cargo --version
  cargo-nextest nextest --version
  cargo-deny --version     | grep -F "0.19.8"
  actionlint --version     | grep -F "1.7.8"
  gitleaks version         | grep -F "8.30.0"
  syft --version           | grep -F "1.40.0"
  jankurai --version       | grep -F "1.5.1"
  node --version           | grep -E "^v20\."
  npm --version
  python3 --version
  sqlite3 -version
  git --version
  jq --version
  flock --version
  [ -z "${RUSTC_WRAPPER}" ] || { echo "RUSTC_WRAPPER is set: ${RUSTC_WRAPPER}" >&2; exit 1; }
  [ "${SCCACHE_DISABLED}" = "1" ] || { echo "SCCACHE_DISABLED not set" >&2; exit 1; }
  echo OK
' >&2 || die "smoke test failed" 2

log "✓ ${IMAGE_REPO}:${PRIMARY_TAG} built and smoke-tested"
