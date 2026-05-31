#!/usr/bin/env bash
# Install or verify the pinned Jankurai binary used by local and hosted gates.
set -euo pipefail

JANKURAI_REPO="${JANKURAI_REPO:-https://github.com/neverhuman/jankurai.git}"
JANKURAI_TAG="${JANKURAI_TAG:-v1.6.10-deadlang-precision}"
JANKURAI_REV="${JANKURAI_REV:-68bd6114373cf407a930011b76669af306cb0cb1}"
JANKURAI_VERSION="${JANKURAI_VERSION:-jankurai 1.6.10}"

strict_tag="${JERYU_JANKURAI_STRICT_TAG:-}"
if [ -z "${strict_tag}" ] && [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  strict_tag=1
fi

if [ "${strict_tag:-0}" = "1" ]; then
  tag_rev="$(git ls-remote --tags "${JANKURAI_REPO}" "refs/tags/${JANKURAI_TAG}" | awk '{print $1}')"
  if [ "${tag_rev}" != "${JANKURAI_REV}" ]; then
    echo "jankurai tag drift: got ${tag_rev} want ${JANKURAI_REV}" >&2
    exit 1
  fi
fi

if command -v jankurai >/dev/null 2>&1 && jankurai --version | grep -qx "${JANKURAI_VERSION}"; then
  echo "jankurai ok: ${JANKURAI_VERSION}"
  exit 0
fi

echo "installing pinned ${JANKURAI_VERSION} from ${JANKURAI_REPO}@${JANKURAI_REV}"
cargo install --git "${JANKURAI_REPO}" --rev "${JANKURAI_REV}" --locked --package jankurai --bin jankurai
jankurai --version | grep -qx "${JANKURAI_VERSION}"
echo "jankurai ok: ${JANKURAI_VERSION}"
