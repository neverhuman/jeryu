#!/usr/bin/env bash
# Release-evidence emitter — single source of release-receipt.json + rollback.json.
#
# Prints a complete release receipt to STDOUT and writes rollback.json into the
# bundle directory. The receipt names, per docs/release.md:
#   * the exact commit SHA
#   * per-artifact SHA-256 digests (binary + SPDX/CycloneDX SBOM + provenance + rollback.json)
#   * the cosign signing transcript
#   * the gate-evidence (required lanes + the jankurai ratchet baseline)
#   * the rollback target: the previous signed release and its binary checksum
#
# rollback.json conforms to jeryu-signrail::RollbackMetadata
# (crates/jeryu-signrail/src/rollback.rs): previous_release / rollback_command /
# config_digest / data_migration / verified_at_epoch.
#
# Honest by construction: every digest is computed from a real file on disk; the
# previous release and its checksum are resolved from the live GitHub releases at
# build time. When that lookup cannot run (offline/headless), the field records
# "unavailable-offline" — it is never invented.
#
# Usage: emit-release-receipt.sh [BUNDLE_DIR]
#   Reads the artifacts release.sh has already assembled into BUNDLE_DIR.
set -euo pipefail

BUNDLE_DIR="${1:-target/release/bundle}"
REPO="${GITHUB_REPOSITORY:-neverhuman/jeryu}"

command -v jq >/dev/null 2>&1 || { echo "[emit-release-receipt] FATAL: jq is required" >&2; exit 1; }

COMMIT="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
RELEASE_TAG="${JERYU_RELEASE_TAG:-${GITHUB_REF_NAME:-$(git describe --tags --exact-match 2>/dev/null || echo unreleased)}}"
NOW_ISO="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
NOW_EPOCH="$(date -u +%s)"

sha_of() { if [ -f "$1" ]; then sha256sum "$1" | awk '{print $1}'; else echo ""; fi; }

BIN_SHA="$(sha_of "${BUNDLE_DIR}/jeryu")"
SPDX_SHA="$(sha_of "${BUNDLE_DIR}/sbom.spdx.json")"
CDX_SHA="$(sha_of "${BUNDLE_DIR}/sbom.cdx.json")"
PROV_SHA="$(sha_of "${BUNDLE_DIR}/provenance.json")"

# --- resolve the previous signed release (best-effort; honest fallback) ------
# The current tag's release is not published yet when this runs, so the most
# recent published, non-prerelease release IS the rollback target.
PREV_TAG=""; PREV_SHA=""
if command -v gh >/dev/null 2>&1; then
  PREV_TAG="$(gh release list --repo "${REPO}" --limit 20 --json tagName,isPrerelease,isDraft 2>/dev/null \
    | jq -r --arg cur "${RELEASE_TAG}" \
      '[.[] | select((.isPrerelease|not) and (.isDraft|not)) | .tagName] | map(select(. != $cur)) | .[0] // ""' \
      2>/dev/null || echo "")"
  if [ -n "${PREV_TAG}" ]; then
    PREV_DIR="${BUNDLE_DIR}/.prev-release"
    mkdir -p "${PREV_DIR}"
    if gh release download "${PREV_TAG}" --repo "${REPO}" --pattern SHA256SUMS --dir "${PREV_DIR}" --clobber >/dev/null 2>&1; then
      PREV_SHA="$(awk '$2=="jeryu"{print $1}' "${PREV_DIR}/SHA256SUMS" 2>/dev/null || echo "")"
    fi
    rm -rf "${PREV_DIR}"
  fi
fi
[ -n "${PREV_TAG}" ] || PREV_TAG="none"
[ -n "${PREV_SHA}" ] || PREV_SHA="unavailable-offline"

# --- rollback.json (jeryu-signrail::RollbackMetadata shape) ------------------
ROLLBACK_CMD="gh release download ${PREV_TAG} --repo ${REPO} --pattern jeryu --pattern jeryu.sig --pattern jeryu.pem && cosign verify-blob --signature jeryu.sig --certificate jeryu.pem --certificate-identity-regexp 'https://github.com/${REPO}/.*release.yml@.*' --certificate-oidc-issuer https://token.actions.githubusercontent.com jeryu && install -m755 jeryu \"\$(command -v jeryu)\""
jq -n \
  --arg prev "${PREV_TAG}" \
  --arg cmd "${ROLLBACK_CMD}" \
  --arg cfg "sha256:${PREV_SHA}" \
  --arg mig "none — release-evidence change only; no SQLite schema or data migration (restore the previous signed binary)" \
  --argjson epoch "${NOW_EPOCH}" \
  '{previous_release: $prev, rollback_command: $cmd, config_digest: $cfg, data_migration: $mig, verified_at_epoch: $epoch}' \
  > "${BUNDLE_DIR}/rollback.json"
ROLLBACK_SHA="$(sha_of "${BUNDLE_DIR}/rollback.json")"

# --- the release receipt (STDOUT) -------------------------------------------
jq -n \
  --arg schema "jeryu.release-receipt/v1" \
  --arg release "${RELEASE_TAG}" \
  --arg commit "${COMMIT}" \
  --arg generated_at "${NOW_ISO}" \
  --arg bin "${BIN_SHA}" \
  --arg spdx "${SPDX_SHA}" \
  --arg cdx "${CDX_SHA}" \
  --arg prov "${PROV_SHA}" \
  --arg rb "${ROLLBACK_SHA}" \
  --arg prev "${PREV_TAG}" \
  --arg prevsha "${PREV_SHA}" \
  '{
    schema: $schema,
    release: $release,
    commit: $commit,
    generated_at: $generated_at,
    artifacts: {
      "jeryu":           {sha256: $bin},
      "sbom.spdx.json":  {sha256: $spdx},
      "sbom.cdx.json":   {sha256: $cdx},
      "provenance.json": {sha256: $prov},
      "rollback.json":   {sha256: $rb}
    },
    provenance: "provenance.json (SLSA in-toto statement over the SBOM digests)",
    cosign_transcript: "cosign.txt",
    gate_evidence: {
      required_lanes: ["ci-fast", "jankurai-audit", "security", "proof-evidence"],
      jankurai_baseline: "agent/baselines/main.repo-score.json (ratchet audit, fail-under 85)",
      sbom_dir: "target/jankurai/security/sbom"
    },
    rollback: {
      previous_release: $prev,
      previous_binary_sha256: $prevsha,
      artifact: "rollback.json"
    },
    generated_by: "scripts/emit-release-receipt.sh"
  }'
