#!/usr/bin/env bash
# Install/verify the open-source tools used by the local and hosted security
# lane. This script is intentionally runnable locally; hosted workflows only
# call this wrapper before `ops/ci/security.sh`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

CARGO_DENY_VERSION="${CARGO_DENY_VERSION:-0.19.8}"
CARGO_AUDIT_VERSION="${CARGO_AUDIT_VERSION:-0.22.1}"
ZIZMOR_VERSION="${ZIZMOR_VERSION:-1.25.2}"
SYFT_VERSION="${SYFT_VERSION:-1.40.0}"
GRYPE_VERSION="${GRYPE_VERSION:-0.99.0}"
COSIGN_VERSION="${COSIGN_VERSION:-2.4.3}"
ACTIONLINT_VERSION="${ACTIONLINT_VERSION:-1.7.8}"

# Pinned release checksums for linux/amd64 artifacts.
SYFT_SHA256="${SYFT_SHA256:-f551cd16da3a5456f5245bb8045b98594263a678a9d2a07b462a05be0357b795}"
GRYPE_SHA256="${GRYPE_SHA256:-62f91dfd7d72c16754a99693cf67f58d0cf6f8b71313c450c2585c7b28891d0c}"
COSIGN_SHA256="${COSIGN_SHA256:-caaad125acef1cb81d58dcdc454a1e429d09a750d1e9e2b3ed1aed8964454708}"
ACTIONLINT_SHA256="${ACTIONLINT_SHA256:-be92c2652ab7b6d08425428797ceabeb16e31a781c07bc388456b4e592f3e36a}"

BINDIR="${HOME}/.local/bin"
mkdir -p "${BINDIR}"
case ":${PATH}:" in
  *":${BINDIR}:"*) ;;
  *) export PATH="${BINDIR}:${PATH}" ;;
esac
if [ -n "${GITHUB_PATH:-}" ]; then
  printf '%s\n' "${BINDIR}" >> "${GITHUB_PATH}"
fi

log() { printf '[security-tools] %s\n' "$*"; }

have_version() {
  local cmd="$1"
  local pattern="$2"
  command -v "${cmd}" >/dev/null 2>&1 && "${cmd}" --version 2>&1 | grep -Eq "${pattern}"
}

have_cosign_version() {
  command -v cosign >/dev/null 2>&1 && cosign version 2>&1 | grep -Eq "GitVersion:[[:space:]]+v${COSIGN_VERSION}"
}

fetch_verify() {
  local url="$1"
  local want="$2"
  local out="$3"
  curl -sSfL "${url}" -o "${out}"
  local got
  got="$(sha256sum "${out}" | awk '{print $1}')"
  if [ "${got}" != "${want}" ]; then
    echo "checksum mismatch for ${url}: got ${got} want ${want}" >&2
    exit 1
  fi
  log "verified ${out} (${got})"
}

install_cargo_bin() {
  local crate="$1"
  local cmd="$2"
  local version="$3"
  local pattern="$4"
  if have_version "${cmd}" "${pattern}"; then
    log "${cmd} ok"
    return 0
  fi
  log "installing ${crate} ${version}"
  cargo install --locked "${crate}" --version "${version}"
}

install_cargo_bin cargo-deny cargo-deny "${CARGO_DENY_VERSION}" "cargo-deny ${CARGO_DENY_VERSION}"
install_cargo_bin cargo-audit cargo-audit "${CARGO_AUDIT_VERSION}" "cargo-audit.*${CARGO_AUDIT_VERSION}"
install_cargo_bin zizmor zizmor "${ZIZMOR_VERSION}" "zizmor ${ZIZMOR_VERSION}"

if ! have_version syft "Version:[[:space:]]+${SYFT_VERSION}|syft ${SYFT_VERSION}"; then
  log "installing syft ${SYFT_VERSION}"
  fetch_verify \
    "https://github.com/anchore/syft/releases/download/v${SYFT_VERSION}/syft_${SYFT_VERSION}_linux_amd64.tar.gz" \
    "${SYFT_SHA256}" \
    /tmp/syft.tgz
  tar -xzf /tmp/syft.tgz -C "${BINDIR}" syft
fi

if ! have_version grype "Version:[[:space:]]+${GRYPE_VERSION}|grype ${GRYPE_VERSION}"; then
  log "installing grype ${GRYPE_VERSION}"
  fetch_verify \
    "https://github.com/anchore/grype/releases/download/v${GRYPE_VERSION}/grype_${GRYPE_VERSION}_linux_amd64.tar.gz" \
    "${GRYPE_SHA256}" \
    /tmp/grype.tgz
  tar -xzf /tmp/grype.tgz -C "${BINDIR}" grype
fi

if have_cosign_version; then
  log "cosign ok"
else
  log "installing cosign ${COSIGN_VERSION}"
  fetch_verify \
    "https://github.com/sigstore/cosign/releases/download/v${COSIGN_VERSION}/cosign-linux-amd64" \
    "${COSIGN_SHA256}" \
    "${BINDIR}/cosign"
  chmod +x "${BINDIR}/cosign"
fi

if ! have_version actionlint "${ACTIONLINT_VERSION}"; then
  log "installing actionlint ${ACTIONLINT_VERSION}"
  fetch_verify \
    "https://github.com/rhysd/actionlint/releases/download/v${ACTIONLINT_VERSION}/actionlint_${ACTIONLINT_VERSION}_linux_amd64.tar.gz" \
    "${ACTIONLINT_SHA256}" \
    /tmp/actionlint.tgz
  tar -xzf /tmp/actionlint.tgz -C "${BINDIR}" actionlint
fi

log "security toolchain ready"
