#!/usr/bin/env bash
# Release-evidence emitter -- single source of release-receipt.json,
# rollback.json, signed-commit proof, and SHA256SUMS.
#
# The final closeout path is intentionally fail-closed. The emitter refuses to
# produce a taggable receipt unless it can verify the candidate commit, resolve
# the previous signed release artifact, read the PR-backed publication metadata,
# and validate the artifact-support SignRail evidence for the same commit and
# rollback target.
#
# Usage: emit-release-receipt.sh [BUNDLE_DIR]
#   Reads the artifacts release.sh has already assembled into BUNDLE_DIR.
#
# Required for final closeout:
#   * a locally verifiable signed commit (`git verify-commit --raw <sha>`)
#   * JERYU_RELEASE_TAG, unless running on an exact tag
#   * rollback evidence from either GitHub releases or:
#       JERYU_RELEASE_ROLLBACK_TAG
#       JERYU_RELEASE_ROLLBACK_SHA256
#       JERYU_RELEASE_ROLLBACK_SIGNATURE_SHA256
#       JERYU_RELEASE_ROLLBACK_CERTIFICATE_SHA256
#   * PR publication metadata at JERYU_RELEASE_PUBLICATION_FILE
#     (default: target/ci-fast/publish.json)
#   * artifact-support SignRail outputs at JERYU_RELEASE_SIGNRAIL_DIR
#     (default: target/artifact-support/signrail)
set -euo pipefail

BUNDLE_DIR="${1:-target/release/bundle}"
REPO="${GITHUB_REPOSITORY:-neverhuman/jeryu}"
PUBLICATION_FILE="${JERYU_RELEASE_PUBLICATION_FILE:-target/ci-fast/publish.json}"
SIGNRAIL_DIR="${JERYU_RELEASE_SIGNRAIL_DIR:-target/artifact-support/signrail}"
ARTIFACT_SUPPORT_BUNDLE="${JERYU_RELEASE_ARTIFACT_SUPPORT_BUNDLE:-target/artifact-support/bundles/artifact-support-evidence.tar.gz}"

command -v jq >/dev/null 2>&1 || {
  echo "[emit-release-receipt] FATAL: jq is required" >&2
  exit 1
}

fail() {
  echo "[emit-release-receipt] FATAL: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  [ -f "$path" ] || fail "missing required file: $path"
}

sha_of_required() {
  local path="$1"
  require_file "$path"
  sha256sum "$path" | awk '{print $1}'
}

sha_of_optional() {
  local path="$1"
  if [ -f "$path" ]; then
    sha256sum "$path" | awk '{print $1}'
  else
    printf ''
  fi
}

validate_sha256_hex() {
  local name="$1" value="$2"
  [[ "$value" =~ ^[0-9a-f]{64}$ ]] || fail "$name must be a 64-hex SHA-256 digest, got '$value'"
}

validate_commit_sha() {
  local value="$1"
  [[ "$value" =~ ^[0-9a-f]{40}$ ]] || fail "commit must be a 40-hex SHA, got '$value'"
}

COMMIT="${JERYU_RELEASE_COMMIT:-$(git rev-parse HEAD 2>/dev/null || true)}"
validate_commit_sha "$COMMIT"
RELEASE_TAG="${JERYU_RELEASE_TAG:-${GITHUB_REF_NAME:-$(git describe --tags --exact-match 2>/dev/null || true)}}"
[ -n "$RELEASE_TAG" ] || fail "JERYU_RELEASE_TAG or an exact release tag is required"
NOW_ISO="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
NOW_EPOCH="$(date -u +%s)"

SIGNED_COMMIT_PROOF="${BUNDLE_DIR}/signed-commit.txt"
verify_signed_commit() {
  local tmp="${SIGNED_COMMIT_PROOF}.tmp"
  if ! git verify-commit --raw "$COMMIT" >"$tmp" 2>&1; then
    cat "$tmp" >&2 || true
    rm -f "$tmp"
    fail "candidate commit $COMMIT is not locally verifiable as a signed commit"
  fi
  {
    printf 'commit: %s\n' "$COMMIT"
    printf 'verified_at: %s\n' "$NOW_ISO"
    cat "$tmp"
  } >"$SIGNED_COMMIT_PROOF"
  rm -f "$tmp"
}

parse_manifest_sha() {
  local manifest="$1" name="$2"
  awk -v want="$name" '($2 == want || $2 == "./" want) {print $1; exit}' "$manifest" 2>/dev/null || true
}

PREV_TAG="${JERYU_RELEASE_ROLLBACK_TAG:-}"
PREV_SHA="${JERYU_RELEASE_ROLLBACK_SHA256:-}"
PREV_SIG_SHA="${JERYU_RELEASE_ROLLBACK_SIGNATURE_SHA256:-}"
PREV_CERT_SHA="${JERYU_RELEASE_ROLLBACK_CERTIFICATE_SHA256:-}"

resolve_previous_signed_release() {
  if [ -n "$PREV_TAG" ] || [ -n "$PREV_SHA" ] || [ -n "$PREV_SIG_SHA" ] || [ -n "$PREV_CERT_SHA" ]; then
    [ -n "$PREV_TAG" ] || fail "JERYU_RELEASE_ROLLBACK_TAG is required with rollback digest overrides"
    [ -n "$PREV_SHA" ] || fail "JERYU_RELEASE_ROLLBACK_SHA256 is required with rollback digest overrides"
    [ -n "$PREV_SIG_SHA" ] || fail "JERYU_RELEASE_ROLLBACK_SIGNATURE_SHA256 is required with rollback digest overrides"
    [ -n "$PREV_CERT_SHA" ] || fail "JERYU_RELEASE_ROLLBACK_CERTIFICATE_SHA256 is required with rollback digest overrides"
  else
    command -v gh >/dev/null 2>&1 || fail "gh is required to resolve the previous signed release, or set JERYU_RELEASE_ROLLBACK_* overrides"
    PREV_TAG="$(gh release list --repo "$REPO" --limit 20 --json tagName,isPrerelease,isDraft 2>/dev/null \
      | jq -r --arg cur "$RELEASE_TAG" \
        '[.[] | select((.isPrerelease|not) and (.isDraft|not)) | .tagName] | map(select(. != $cur)) | .[0] // ""' \
        2>/dev/null || true)"
    [ -n "$PREV_TAG" ] || fail "could not resolve a previous published non-draft signed release for $REPO"

    local prev_dir="${BUNDLE_DIR}/.prev-release"
    rm -rf "$prev_dir"
    mkdir -p "$prev_dir"
    if ! gh release download "$PREV_TAG" --repo "$REPO" --pattern SHA256SUMS --dir "$prev_dir" --clobber >/dev/null 2>&1; then
      rm -rf "$prev_dir"
      fail "could not download SHA256SUMS for previous release $PREV_TAG"
    fi
    PREV_SHA="$(parse_manifest_sha "$prev_dir/SHA256SUMS" jeryu)"
    PREV_SIG_SHA="$(parse_manifest_sha "$prev_dir/SHA256SUMS" jeryu.sig)"
    PREV_CERT_SHA="$(parse_manifest_sha "$prev_dir/SHA256SUMS" jeryu.pem)"
    rm -rf "$prev_dir"
  fi

  [ "$PREV_TAG" != "$RELEASE_TAG" ] || fail "rollback target must not be the release being built ($RELEASE_TAG)"
  validate_sha256_hex "previous binary checksum" "$PREV_SHA"
  validate_sha256_hex "previous binary signature checksum" "$PREV_SIG_SHA"
  validate_sha256_hex "previous binary certificate checksum" "$PREV_CERT_SHA"
}

PUB_MODE=""
PUB_BRANCH=""
PUB_BASE=""
PUB_URL=""
PUB_NUMBER=""
PUB_COMMIT=""
PUB_DIRECT_ESCAPE="false"

validate_publication() {
  require_file "$PUBLICATION_FILE"
  jq -e . "$PUBLICATION_FILE" >/dev/null || fail "publication metadata is not valid JSON: $PUBLICATION_FILE"
  PUB_MODE="$(jq -r '.mode // ""' "$PUBLICATION_FILE")"
  PUB_BRANCH="$(jq -r '.branch // ""' "$PUBLICATION_FILE")"
  PUB_BASE="$(jq -r '.base // ""' "$PUBLICATION_FILE")"
  PUB_URL="$(jq -r '.pr.url // .pr_url // ""' "$PUBLICATION_FILE")"
  PUB_NUMBER="$(jq -r '(.pr.number // .pr_number // "") | tostring' "$PUBLICATION_FILE")"
  PUB_COMMIT="$(jq -r '.commit // ""' "$PUBLICATION_FILE")"
  PUB_DIRECT_ESCAPE="$(jq -r '.direct_main_escape // false' "$PUBLICATION_FILE")"

  [ -n "$PUB_BRANCH" ] || fail "publication metadata missing branch"
  [ -n "$PUB_BASE" ] || fail "publication metadata missing base"
  if [ -n "$PUB_COMMIT" ] && [ "$PUB_COMMIT" != "$COMMIT" ]; then
    fail "publication metadata commit $PUB_COMMIT does not match release commit $COMMIT"
  fi

  case "$PUB_MODE" in
    pr)
      [ "$PUB_BRANCH" != "main" ] && [ "$PUB_BRANCH" != "master" ] || fail "PR publication branch must not be $PUB_BRANCH"
      [ -n "$PUB_URL" ] || fail "PR publication metadata missing pr.url"
      [ -n "$PUB_NUMBER" ] || fail "PR publication metadata missing pr.number"
      ;;
    direct-main)
      [ "${JERYU_RELEASE_DIRECT_MAIN_ESCAPE:-0}" = "1" ] || fail "direct-main publication metadata is an escape hatch; set JERYU_RELEASE_DIRECT_MAIN_ESCAPE=1 to use it"
      PUB_DIRECT_ESCAPE="true"
      ;;
    *)
      fail "publication metadata mode must be 'pr', got '$PUB_MODE'"
      ;;
  esac
}

SIGNRAIL_RELEASE_SHA=""
SIGNRAIL_SBOM_SHA=""
SIGNRAIL_PROVENANCE_SHA=""
SIGNRAIL_WITNESS_SHA=""
SIGNRAIL_LOCAL_SHA=""
SIGNRAIL_DEV_SHA=""
SIGNRAIL_PROD_SHA=""
SIGNRAIL_SUMMARY_SHA=""
SIGNRAIL_STAGE_ARTIFACT_DIGEST=""
ARTIFACT_SUPPORT_SHA=""

validate_signrail_outputs() {
  require_file "$ARTIFACT_SUPPORT_BUNDLE"
  ARTIFACT_SUPPORT_SHA="$(sha256sum "$ARTIFACT_SUPPORT_BUNDLE" | awk '{print $1}')"
  validate_sha256_hex "artifact-support bundle checksum" "$ARTIFACT_SUPPORT_SHA"

  for file in release.json sbom.json provenance.json witness.json; do
    require_file "$SIGNRAIL_DIR/$file"
    jq -e . "$SIGNRAIL_DIR/$file" >/dev/null || fail "SignRail $file is not valid JSON"
  done
  require_file "$SIGNRAIL_DIR/stage-receipts/local.json"
  require_file "$SIGNRAIL_DIR/stage-receipts/dev-canary.json"
  require_file "$SIGNRAIL_DIR/stage-receipts/prod.json"

  jq -e --arg commit "$COMMIT" '.commit_sha == $commit' "$SIGNRAIL_DIR/release.json" >/dev/null \
    || fail "SignRail release.json commit does not match $COMMIT"
  jq -e --arg rollback "$PREV_TAG" '.rollback.previous_release == $rollback' "$SIGNRAIL_DIR/release.json" >/dev/null \
    || fail "SignRail rollback target does not match previous release $PREV_TAG"
  jq -e '.signature_coverage_percent == 100' "$SIGNRAIL_DIR/witness.json" >/dev/null \
    || fail "SignRail witness does not prove 100% signature coverage"

  local stage artifact_digest
  for stage in local dev-canary prod; do
    local receipt="$SIGNRAIL_DIR/stage-receipts/${stage}.json"
    jq -e . "$receipt" >/dev/null || fail "SignRail stage receipt is not valid JSON: $receipt"
    jq -e --arg stage "$stage" --arg commit "$COMMIT" --arg rollback "$PREV_TAG" \
      '.payload.stage == $stage and .payload.sha == $commit and .payload.rollback_target == $rollback and .payload.signature_coverage_percent == 100' \
      "$receipt" >/dev/null || fail "SignRail $stage stage receipt disagrees with commit, rollback target, or signature coverage"
    artifact_digest="$(jq -r '.payload.artifact_digest // ""' "$receipt")"
    [[ "$artifact_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || fail "SignRail $stage artifact digest is not sha256-tagged"
    if [ -z "$SIGNRAIL_STAGE_ARTIFACT_DIGEST" ]; then
      SIGNRAIL_STAGE_ARTIFACT_DIGEST="$artifact_digest"
    elif [ "$SIGNRAIL_STAGE_ARTIFACT_DIGEST" != "$artifact_digest" ]; then
      fail "SignRail stage artifact digests do not agree"
    fi
  done

  SIGNRAIL_RELEASE_SHA="$(sha256sum "$SIGNRAIL_DIR/release.json" | awk '{print $1}')"
  SIGNRAIL_SBOM_SHA="$(sha256sum "$SIGNRAIL_DIR/sbom.json" | awk '{print $1}')"
  SIGNRAIL_PROVENANCE_SHA="$(sha256sum "$SIGNRAIL_DIR/provenance.json" | awk '{print $1}')"
  SIGNRAIL_WITNESS_SHA="$(sha256sum "$SIGNRAIL_DIR/witness.json" | awk '{print $1}')"
  SIGNRAIL_LOCAL_SHA="$(sha256sum "$SIGNRAIL_DIR/stage-receipts/local.json" | awk '{print $1}')"
  SIGNRAIL_DEV_SHA="$(sha256sum "$SIGNRAIL_DIR/stage-receipts/dev-canary.json" | awk '{print $1}')"
  SIGNRAIL_PROD_SHA="$(sha256sum "$SIGNRAIL_DIR/stage-receipts/prod.json" | awk '{print $1}')"
  SIGNRAIL_SUMMARY_SHA="$(sha_of_optional "$SIGNRAIL_DIR/summary.json")"
}

write_rollback_json() {
  local rollback_cmd
  rollback_cmd="gh release download ${PREV_TAG} --repo ${REPO} --pattern jeryu --pattern jeryu.sig --pattern jeryu.pem && cosign verify-blob --signature jeryu.sig --certificate jeryu.pem --certificate-identity-regexp 'https://github.com/${REPO}/.*release.yml@.*' --certificate-oidc-issuer https://token.actions.githubusercontent.com jeryu && install -m755 jeryu \"\$(command -v jeryu)\""
  jq -n \
    --arg prev "$PREV_TAG" \
    --arg cmd "$rollback_cmd" \
    --arg cfg "sha256:${PREV_SHA}" \
    --arg mig "none - release-evidence change only; no SQLite schema or data migration (restore the previous signed binary)" \
    --argjson epoch "$NOW_EPOCH" \
    '{previous_release: $prev, rollback_command: $cmd, config_digest: $cfg, data_migration: $mig, verified_at_epoch: $epoch}' \
    >"${BUNDLE_DIR}/rollback.json"
}

write_checksum_manifest() {
  (
    cd "$BUNDLE_DIR"
    find . -type f \
      ! -path './release-receipt.json' \
      ! -path './SHA256SUMS' \
      ! -path './.prev-release/*' \
      -printf '%P\0' \
      | LC_ALL=C sort -z \
      | xargs -0 -r sha256sum > SHA256SUMS
  )
}

require_file "${BUNDLE_DIR}/jeryu"
require_file "${BUNDLE_DIR}/sbom.spdx.json"
require_file "${BUNDLE_DIR}/sbom.cdx.json"
require_file "${BUNDLE_DIR}/provenance.json"
require_file "${BUNDLE_DIR}/cosign.txt"

verify_signed_commit
resolve_previous_signed_release
validate_publication
validate_signrail_outputs
write_rollback_json

BIN_SHA="$(sha_of_required "${BUNDLE_DIR}/jeryu")"
SPDX_SHA="$(sha_of_required "${BUNDLE_DIR}/sbom.spdx.json")"
CDX_SHA="$(sha_of_required "${BUNDLE_DIR}/sbom.cdx.json")"
PROV_SHA="$(sha_of_required "${BUNDLE_DIR}/provenance.json")"
COSIGN_SHA="$(sha_of_required "${BUNDLE_DIR}/cosign.txt")"
ROLLBACK_SHA="$(sha_of_required "${BUNDLE_DIR}/rollback.json")"
SIGNED_COMMIT_SHA="$(sha_of_required "$SIGNED_COMMIT_PROOF")"
GRYPE_SHA="$(sha_of_optional "${BUNDLE_DIR}/grype-scan.json")"
SIG_SHA="$(sha_of_optional "${BUNDLE_DIR}/jeryu.sig")"
PEM_SHA="$(sha_of_optional "${BUNDLE_DIR}/jeryu.pem")"
SIG_NOTE_SHA="$(sha_of_optional "${BUNDLE_DIR}/jeryu.sig.note")"

jq -e --arg commit "$COMMIT" '.predicate.buildDefinition.externalParameters.sourceCommit == $commit' "${BUNDLE_DIR}/provenance.json" >/dev/null \
  || fail "provenance.json sourceCommit does not match $COMMIT"

write_checksum_manifest
CHECKSUMS_SHA="$(sha_of_required "${BUNDLE_DIR}/SHA256SUMS")"

jq -n \
  --arg schema "jeryu.release-receipt/v2" \
  --arg release "$RELEASE_TAG" \
  --arg commit "$COMMIT" \
  --arg generated_at "$NOW_ISO" \
  --arg bin "$BIN_SHA" \
  --arg spdx "$SPDX_SHA" \
  --arg cdx "$CDX_SHA" \
  --arg prov "$PROV_SHA" \
  --arg cosign "$COSIGN_SHA" \
  --arg rb "$ROLLBACK_SHA" \
  --arg signed_commit "$SIGNED_COMMIT_SHA" \
  --arg grype "$GRYPE_SHA" \
  --arg sig "$SIG_SHA" \
  --arg pem "$PEM_SHA" \
  --arg sig_note "$SIG_NOTE_SHA" \
  --arg checksums "$CHECKSUMS_SHA" \
  --arg prev "$PREV_TAG" \
  --arg prevsha "$PREV_SHA" \
  --arg prevsig "$PREV_SIG_SHA" \
  --arg prevcert "$PREV_CERT_SHA" \
  --arg pub_mode "$PUB_MODE" \
  --arg pub_branch "$PUB_BRANCH" \
  --arg pub_base "$PUB_BASE" \
  --arg pub_url "$PUB_URL" \
  --arg pub_number "$PUB_NUMBER" \
  --arg pub_commit "$PUB_COMMIT" \
  --arg pub_file "$PUBLICATION_FILE" \
  --arg pub_escape "$PUB_DIRECT_ESCAPE" \
  --arg support_bundle "$ARTIFACT_SUPPORT_SHA" \
  --arg support_digest "$SIGNRAIL_STAGE_ARTIFACT_DIGEST" \
  --arg signrail_release "$SIGNRAIL_RELEASE_SHA" \
  --arg signrail_sbom "$SIGNRAIL_SBOM_SHA" \
  --arg signrail_prov "$SIGNRAIL_PROVENANCE_SHA" \
  --arg signrail_witness "$SIGNRAIL_WITNESS_SHA" \
  --arg signrail_local "$SIGNRAIL_LOCAL_SHA" \
  --arg signrail_dev "$SIGNRAIL_DEV_SHA" \
  --arg signrail_prod "$SIGNRAIL_PROD_SHA" \
  --arg signrail_summary "$SIGNRAIL_SUMMARY_SHA" \
  '{
    schema: $schema,
    release: $release,
    commit: $commit,
    generated_at: $generated_at,
    artifacts: ({
      "jeryu":           {sha256: $bin},
      "sbom.spdx.json":  {sha256: $spdx},
      "sbom.cdx.json":   {sha256: $cdx},
      "provenance.json": {sha256: $prov},
      "cosign.txt":      {sha256: $cosign},
      "rollback.json":   {sha256: $rb},
      "signed-commit.txt": {sha256: $signed_commit},
      "SHA256SUMS":      {sha256: $checksums}
    }
    + (if $grype == "" then {} else {"grype-scan.json": {sha256: $grype}} end)
    + (if $sig == "" then {} else {"jeryu.sig": {sha256: $sig}} end)
    + (if $pem == "" then {} else {"jeryu.pem": {sha256: $pem}} end)
    + (if $sig_note == "" then {} else {"jeryu.sig.note": {sha256: $sig_note}} end)),
    signed_commit: {
      verified: true,
      proof: "signed-commit.txt",
      proof_sha256: $signed_commit
    },
    publication: {
      mode: $pub_mode,
      branch: $pub_branch,
      base: $pub_base,
      pr: {
        url: $pub_url,
        number: $pub_number
      },
      commit: (if $pub_commit == "" then $commit else $pub_commit end),
      metadata_file: $pub_file,
      direct_main_escape: ($pub_escape == "true")
    },
    provenance: {
      path: "provenance.json",
      predicate_type: "https://slsa.dev/provenance/v1",
      source_commit: $commit
    },
    cosign_transcript: "cosign.txt",
    checksum_manifest: {
      path: "SHA256SUMS",
      sha256: $checksums,
      excludes: ["release-receipt.json", "SHA256SUMS"]
    },
    gate_evidence: {
      required_lanes: ["ci-fast-full-local", "ci-fast-full-github-clean", "jankurai-audit", "security", "proof-evidence", "release", "artifact-support"],
      local_full_command: "bash ci-fast-push.sh --full --no-push",
      github_clean_full_command: "JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push",
      jankurai_baseline: "agent/baselines/main.repo-score.json (ratchet audit, fail-under 85)",
      sbom_dir: "target/jankurai/security/sbom"
    },
    rollback: {
      previous_release: $prev,
      previous_binary_sha256: $prevsha,
      previous_signature_sha256: $prevsig,
      previous_certificate_sha256: $prevcert,
      artifact: "rollback.json"
    },
    artifact_support: ({
      bundle: "artifact-support-evidence.tar.gz",
      bundle_sha256: $support_bundle,
      signed_artifact_digest: $support_digest,
      signrail_dir: "artifact-support-signrail",
      files: {
        "release.json": {sha256: $signrail_release},
        "sbom.json": {sha256: $signrail_sbom},
        "provenance.json": {sha256: $signrail_prov},
        "witness.json": {sha256: $signrail_witness},
        "stage-receipts/local.json": {sha256: $signrail_local},
        "stage-receipts/dev-canary.json": {sha256: $signrail_dev},
        "stage-receipts/prod.json": {sha256: $signrail_prod}
      }
    } + (if $signrail_summary == "" then {} else {summary_sha256: $signrail_summary} end)),
    generated_by: "scripts/emit-release-receipt.sh"
  }'
