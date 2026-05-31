#!/usr/bin/env bash
# Provision and pin sccache for the jeryu CI cache lane. This mirrors the
# pinned-tool pattern in ops/ci/security-tools.sh: a fixed version, a pinned
# sha256, an official upstream release artifact, and a post-install version
# probe.
#
# CRITICAL: the native runner sandbox PATH is /usr/local/bin:/usr/bin:/bin, so
# the sccache binary MUST land in /usr/local/bin (not ~/.cargo/bin) to be
# visible to the cache lane. Installing elsewhere would silently disable the
# opportunistic RUSTC_WRAPPER=sccache path in ops/ci/ci-env.sh.
#
# By default this script is opportunistic: a download/install failure is logged
# but does not abort, so non-cache lanes keep working without the binary. Set
# JERYU_CI_REQUIRE_SCCACHE=1 to hard-fail when the pinned binary is absent
# (used by the cache e2e lane).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

SCCACHE_VERSION="${SCCACHE_VERSION:-0.8.2}"
SCCACHE_TARGET="${SCCACHE_TARGET:-x86_64-unknown-linux-musl}"

# Pinned release checksum for the official upstream tarball
# (sccache-v0.8.2-x86_64-unknown-linux-musl.tar.gz). Override SCCACHE_SHA256
# when bumping SCCACHE_VERSION / SCCACHE_TARGET; the published sidecar is at
# <tarball-url>.sha256.
SCCACHE_SHA256="${SCCACHE_SHA256:-ecda4ddc89a49f1ec6f35bdce5ecbf6f205b399a680d11119d4ce9f6d962104e}"

# The cache lane requires sccache on the sandbox PATH, which is
# /usr/local/bin:/usr/bin:/bin. Keep this as the install target.
INSTALL_DIR="${SCCACHE_INSTALL_DIR:-/usr/local/bin}"
INSTALL_PATH="${INSTALL_DIR}/sccache"

REQUIRE="${JERYU_CI_REQUIRE_SCCACHE:-0}"

log() { printf '[cache-tools] %s\n' "$*"; }

# fail_or_skip <message>
#
# In require mode, abort hard. Otherwise log the limitation and exit cleanly so
# opportunistic lanes are unaffected.
fail_or_skip() {
  local msg="$1"
  if [ "${REQUIRE}" = "1" ]; then
    echo "[cache-tools] FATAL: ${msg} (JERYU_CI_REQUIRE_SCCACHE=1)" >&2
    exit 1
  fi
  log "skip: ${msg} (opportunistic; set JERYU_CI_REQUIRE_SCCACHE=1 to enforce)"
  exit 0
}

# Run a privileged install step. /usr/local/bin is typically root-owned, so use
# sudo when the directory is not writable by the current user. Falls back to a
# direct (non-sudo) invocation when already writable or running as root.
priv() {
  if [ -w "${INSTALL_DIR}" ] || [ "$(id -u)" = "0" ]; then
    "$@"
  elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    sudo "$@"
  else
    return 87
  fi
}

have_pinned_sccache() {
  command -v sccache >/dev/null 2>&1 || return 1
  # Resolve to the install-target binary so we don't accept a stray sccache from
  # ~/.cargo/bin that the sandbox PATH cannot see.
  [ -x "${INSTALL_PATH}" ] || return 1
  "${INSTALL_PATH}" --version 2>/dev/null | grep -Eq "sccache[[:space:]]+(v)?${SCCACHE_VERSION}\b"
}

fetch_verify() {
  local url="$1"
  local want="$2"
  local out="$3"
  if ! curl -sSfL "${url}" -o "${out}"; then
    return 1
  fi
  local got
  got="$(sha256sum "${out}" | awk '{print $1}')"
  if [ "${got}" != "${want}" ]; then
    echo "[cache-tools] checksum mismatch for ${url}: got ${got} want ${want}" >&2
    return 2
  fi
  log "verified ${out} (${got})"
}

if have_pinned_sccache; then
  log "sccache ${SCCACHE_VERSION} already installed at ${INSTALL_PATH}"
  "${INSTALL_PATH}" --version
  log "cache toolchain ready"
  exit 0
fi

ASSET="sccache-v${SCCACHE_VERSION}-${SCCACHE_TARGET}"
URL="https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/${ASSET}.tar.gz"

WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

log "installing sccache ${SCCACHE_VERSION} (${SCCACHE_TARGET}) -> ${INSTALL_PATH}"
if ! fetch_verify "${URL}" "${SCCACHE_SHA256}" "${WORK}/sccache.tgz"; then
  fail_or_skip "could not download/verify ${URL}"
fi

if ! tar -xzf "${WORK}/sccache.tgz" -C "${WORK}" "${ASSET}/sccache"; then
  fail_or_skip "could not extract sccache from ${WORK}/sccache.tgz"
fi
chmod +x "${WORK}/${ASSET}/sccache"

if ! priv install -m 0755 "${WORK}/${ASSET}/sccache" "${INSTALL_PATH}"; then
  fail_or_skip "could not install sccache to ${INSTALL_PATH} (need write access or passwordless sudo)"
fi

if ! have_pinned_sccache; then
  fail_or_skip "installed sccache at ${INSTALL_PATH} but version probe failed"
fi

"${INSTALL_PATH}" --version
log "cache toolchain ready"
