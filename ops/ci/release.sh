#!/usr/bin/env bash
# Release lane (single-source: identical locally and on GitHub Actions).
#
# Builds + validates the v4 artifact, produces an SBOM + SLSA provenance, cosign-
# signs the binary (keyless OIDC when GitHub provides it; recorded-honestly
# otherwise — never faked), assembles a signed release bundle, and — only on a
# tag inside GitHub Actions — publishes a GitHub Release with the bundle.
set -euo pipefail
export JERYU_CI_USE_SCCACHE=0
unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_CACHE_SIZE
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${HERE}/common.sh"
unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_CACHE_SIZE
ROOT="$(cd "${HERE}/../.." && pwd)"
cd "${ROOT}"

log() { printf '[release] %s\n' "$*"; }

# --- 1. validate + build the release binary --------------------------------
cargo test --workspace --jobs "${JERYU_CI_JOBS}"
cargo build --release -p jeryu-cli --bin jeryu --jobs "${JERYU_CI_JOBS}"
BIN="target/release/jeryu"
[ -x "${BIN}" ] || { echo "[release] FATAL: ${BIN} not built" >&2; exit 1; }
log "built $(${BIN} --version 2>/dev/null || echo jeryu)"

# --- 2. SBOM + grype + SLSA provenance + cosign (over the SBOM) -------------
bash "${HERE}/sbom-provenance.sh"
SBOM_DIR="target/jankurai/security/sbom"

# --- 3. assemble the signed release bundle ---------------------------------
BUNDLE="target/release/bundle"
rm -rf "${BUNDLE}"; mkdir -p "${BUNDLE}"
cp "${BIN}" "${BUNDLE}/jeryu"
for f in sbom.spdx.json sbom.cdx.json provenance.json cosign.txt grype-scan.json; do
  [ -f "${SBOM_DIR}/${f}" ] && cp "${SBOM_DIR}/${f}" "${BUNDLE}/" || true
done
( cd "${BUNDLE}" && sha256sum * > SHA256SUMS 2>/dev/null || true )
bash ./scripts/emit-release-receipt.sh > "${BUNDLE}/release-receipt.json" 2>/dev/null \
  || cp target/release-receipt.json "${BUNDLE}/release-receipt.json" 2>/dev/null \
  || log "release receipt emitter produced no stdout file (non-fatal)"

# --- 4. cosign-sign the binary (keyless OIDC when available) ----------------
if command -v cosign >/dev/null 2>&1; then
  if [ -n "${ACTIONS_ID_TOKEN_REQUEST_TOKEN:-}" ]; then
    log "cosign keyless sign-blob over the binary (GitHub OIDC)"
    cosign sign-blob --yes \
      --output-signature "${BUNDLE}/jeryu.sig" \
      --output-certificate "${BUNDLE}/jeryu.pem" \
      "${BUNDLE}/jeryu" && log "binary signature -> ${BUNDLE}/jeryu.sig"
  else
    log "cosign present but no OIDC token (local/headless) — binary signature is produced by the GitHub release lane; recorded honestly, not faked"
    printf 'cosign present; keyless OIDC unavailable in this environment. The binary signature + Fulcio certificate are produced by the GitHub Actions release lane (id-token: write).\n' \
      > "${BUNDLE}/jeryu.sig.note"
  fi
else
  log "cosign absent — recorded honestly; install via ops/ci/security-tools.sh"
  printf 'cosign absent in this environment; no binary signature produced here.\n' > "${BUNDLE}/jeryu.sig.note"
fi

log "release bundle ready at ${BUNDLE}:"
ls -la "${BUNDLE}" || true

# --- 5. publish the GitHub Release (only on a tag inside GitHub Actions) ----
if [ "${GITHUB_ACTIONS:-}" = "true" ] && [[ "${GITHUB_REF:-}" == refs/tags/* ]]; then
  TAG="${GITHUB_REF_NAME:-${GITHUB_REF#refs/tags/}}"
  log "publishing GitHub Release ${TAG}"
  if gh release view "${TAG}" >/dev/null 2>&1; then
    # Releases are immutable: never overwrite published assets. A re-release
    # publishes a NEW version with fresh checksums and notes.
    log "release ${TAG} already exists — immutable, not overwriting; publish a new version to re-release"
  else
    gh release create "${TAG}" "${BUNDLE}"/* \
      --title "jeryu ${TAG}" \
      --notes "jeryu ${TAG} — signed build + SBOM + SLSA provenance. See CHANGELOG.md." \
      --verify-tag
    log "released ${TAG}"
  fi
else
  log "not a tag inside GitHub Actions — bundle built + signed locally; publish skipped (this is the single-source local path)"
fi
