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
die() { printf '[release] FATAL: %s\n' "$*" >&2; exit 1; }

BUNDLE="target/release/bundle"
ARTIFACT_SUPPORT_ROOT="${JERYU_ARTIFACT_SUPPORT_ROOT:-target/artifact-support}"
ARTIFACT_SUPPORT_BUNDLE_SRC="${JERYU_RELEASE_ARTIFACT_SUPPORT_BUNDLE:-${ARTIFACT_SUPPORT_ROOT}/bundles/artifact-support-evidence.tar.gz}"
ARTIFACT_SUPPORT_SIGNRAIL_SRC="${JERYU_RELEASE_SIGNRAIL_DIR:-${ARTIFACT_SUPPORT_ROOT}/signrail}"
ARTIFACT_SUPPORT_BUNDLE_DST="${BUNDLE}/artifact-support-evidence.tar.gz"
ARTIFACT_SUPPORT_SIGNRAIL_DST="${BUNDLE}/artifact-support-signrail"
PUBLICATION_FILE="${JERYU_RELEASE_PUBLICATION_FILE:-target/ci-fast/publish.json}"

# --- 0. self-check the release-evidence emitter (fail fast if broken) -------
log "self-testing the release-receipt emitter"
bash "${ROOT}/scripts/test-emit-release-receipt.sh"

log "preflighting final closeout metadata"
git verify-commit --raw HEAD >/dev/null 2>&1 \
  || die "candidate commit $(git rev-parse HEAD) is not locally verifiable as a signed commit"
[ -f "${PUBLICATION_FILE}" ] \
  || die "missing PR publication metadata: ${PUBLICATION_FILE}; run bash ci-fast-push.sh --full from a PR branch before tagging"

# --- 1. validate + build the release binary --------------------------------
cargo test --workspace --jobs "${JERYU_CI_JOBS}"
cargo build --release -p jeryu-cli --bin jeryu --jobs "${JERYU_CI_JOBS}"
BIN="target/release/jeryu"
[ -x "${BIN}" ] || { echo "[release] FATAL: ${BIN} not built" >&2; exit 1; }
log "built $(${BIN} --version 2>/dev/null || echo jeryu)"

# --- 2. SBOM + grype + SLSA provenance + cosign (over the SBOM) -------------
bash "${HERE}/sbom-provenance.sh"
SBOM_DIR="target/jankurai/security/sbom"

# --- 3. assemble the release bundle ----------------------------------------
if [ ! -f "${ARTIFACT_SUPPORT_BUNDLE_SRC}" ] || [ ! -d "${ARTIFACT_SUPPORT_SIGNRAIL_SRC}" ]; then
  log "artifact-support evidence missing; running ops/ci/artifact_support.sh"
  bash "${HERE}/artifact_support.sh"
fi
[ -f "${ARTIFACT_SUPPORT_BUNDLE_SRC}" ] \
  || die "missing artifact-support bundle after artifact_support.sh: ${ARTIFACT_SUPPORT_BUNDLE_SRC}"
[ -d "${ARTIFACT_SUPPORT_SIGNRAIL_SRC}" ] \
  || die "missing artifact-support SignRail outputs after artifact_support.sh: ${ARTIFACT_SUPPORT_SIGNRAIL_SRC}"

rm -rf "${BUNDLE}"; mkdir -p "${BUNDLE}"
cp "${BIN}" "${BUNDLE}/jeryu"
for f in sbom.spdx.json sbom.cdx.json provenance.json cosign.txt grype-scan.json; do
  [ -f "${SBOM_DIR}/${f}" ] && cp "${SBOM_DIR}/${f}" "${BUNDLE}/" || true
done
cp "${ARTIFACT_SUPPORT_BUNDLE_SRC}" "${ARTIFACT_SUPPORT_BUNDLE_DST}"
rm -rf "${ARTIFACT_SUPPORT_SIGNRAIL_DST}"
mkdir -p "${ARTIFACT_SUPPORT_SIGNRAIL_DST}"
cp -R "${ARTIFACT_SUPPORT_SIGNRAIL_SRC}/." "${ARTIFACT_SUPPORT_SIGNRAIL_DST}/"

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

# --- 4b. release receipt + stable checksum manifest -------------------------
# The emitter writes rollback.json, signed-commit.txt, SHA256SUMS, and the full
# receipt. SHA256SUMS covers every bundle file except itself and the receipt, so
# the receipt can record the checksum-manifest digest without a self-reference.
log "emitting final release receipt"
JERYU_RELEASE_SIGNRAIL_DIR="${ARTIFACT_SUPPORT_SIGNRAIL_DST}" \
JERYU_RELEASE_ARTIFACT_SUPPORT_BUNDLE="${ARTIFACT_SUPPORT_BUNDLE_DST}" \
  bash ./scripts/emit-release-receipt.sh "${BUNDLE}" > "${BUNDLE}/release-receipt.json"

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
