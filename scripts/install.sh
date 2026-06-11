#!/usr/bin/env bash
set -euo pipefail

repo="neverhuman/jeryu-deploy"
version="${JERYU_VERSION:-latest}"
install_dir="${JERYU_INSTALL_DIR:-${HOME}/.jeryu/bin}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if [[ "$version" == "latest" ]]; then
  base="https://github.com/${repo}/releases/latest/download"
else
  base="https://github.com/${repo}/releases/download/${version}"
fi

download() {
  local name="$1"
  curl -fL --retry 3 --retry-delay 2 -o "${tmp}/${name}" "${base}/${name}"
}

download jeryu
download SHA256SUMS

(
  cd "$tmp"
  grep -Eq '([[:space:]]|\*)jeryu$' SHA256SUMS || {
    printf 'SHA256SUMS does not contain a jeryu entry\n' >&2
    exit 1
  }
  sha256sum --check --ignore-missing SHA256SUMS
)

if command -v cosign >/dev/null 2>&1; then
  sig_ok=0
  if curl -fL --retry 3 --retry-delay 2 -o "${tmp}/jeryu.sig" "${base}/jeryu.sig"; then
    if curl -fL --retry 3 --retry-delay 2 -o "${tmp}/jeryu.pem" "${base}/jeryu.pem"; then
      sig_ok=1
    fi
  fi
  if [[ "$sig_ok" == "1" ]]; then
    cosign verify-blob       --signature "${tmp}/jeryu.sig"       --certificate "${tmp}/jeryu.pem"       --certificate-identity-regexp "https://github.com/${repo}/.*release.yml@.*"       --certificate-oidc-issuer "https://token.actions.githubusercontent.com"       "${tmp}/jeryu"
  else
    printf 'cosign assets unavailable; SHA256SUMS verification completed\n' >&2
  fi
else
  printf 'cosign not found; SHA256SUMS verification completed\n' >&2
fi

mkdir -p "$install_dir"
install -m 0755 "${tmp}/jeryu" "${install_dir}/jeryu"
printf 'installed jeryu to %s\n' "${install_dir}/jeryu"
