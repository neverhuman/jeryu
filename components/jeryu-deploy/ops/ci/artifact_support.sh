#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "${ROOT}/ops/ci/lib.sh"
cd "${ROOT}"

ARTIFACT_ROOT="${JERYU_ARTIFACT_SUPPORT_ROOT:-target/artifact-support}"
EVIDENCE_DIR="${ARTIFACT_ROOT}/evidence"
BUNDLE_DIR="${ARTIFACT_ROOT}/bundles"
SIGNRAIL_DIR="${JERYU_RELEASE_SIGNRAIL_DIR:-${ARTIFACT_ROOT}/signrail}"
SIGNRAIL_MANIFEST="${JERYU_SIGNRAIL_MANIFEST:-${ROOT}/../jeryu-release-ops/Cargo.toml}"
SIGNRAIL_STORE_ROOT="${SIGNRAIL_STORE_ROOT:-${HOME}/.local/share/jeryu/signrail}"
BUNDLE="${JERYU_RELEASE_ARTIFACT_SUPPORT_BUNDLE:-${BUNDLE_DIR}/artifact-support-evidence.tar.gz}"
BINARY="${JERYU_RELEASE_BINARY:-target/release/jeryu}"
ROUTE_PROBE="${JERYU_RELEASE_ROUTE_PROBE_RECEIPT:-${ARTIFACT_ROOT}/route-probe/receipt.json}"
STRICT="${JERYU_ARTIFACT_SUPPORT_STRICT:-0}"

log() { printf '[artifact-support] %s\n' "$*"; }
die() { printf '[artifact-support] FATAL: %s\n' "$*" >&2; exit 1; }

sha_file() {
  if [ -f "$1" ]; then
    sha256sum "$1" | awk '{print $1}'
  else
    printf ''
  fi
}

sha_text() {
  printf '%s' "$1" | sha256sum | awk '{print $1}'
}

require_strict() {
  if [ "${STRICT}" = "1" ]; then
    die "$1"
  fi
  log "$1; writing unsigned non-release evidence"
}

mkdir -p "${EVIDENCE_DIR}" "${BUNDLE_DIR}" "${SIGNRAIL_DIR}"
rm -rf "${EVIDENCE_DIR:?}/"*

COMMIT="$(git rev-parse HEAD)"
TREE="$(git rev-parse 'HEAD^{tree}')"
REPO="${GITHUB_REPOSITORY:-neverhuman/jeryu-deploy}"
VERSION="${JERYU_RELEASE_TAG:-${GITHUB_REF_NAME:-${COMMIT}}}"
ROLLBACK_TARGET="${SIGNRAIL_ROLLBACK_TARGET:-${JERYU_RELEASE_ROLLBACK_TAG:-}}"
MANIFEST_SHA="$(sha_file repos.manifest.toml)"
LOCK_SHA="$(sha_file jeryu-split.lock.toml)"
CARGO_LOCK_SHA="$(sha_file Cargo.lock)"
TOOLCHAIN_TEXT="$(rustc -Vv 2>/dev/null || printf 'rustc not found')"
TOOLCHAIN_SHA="$(sha_text "${TOOLCHAIN_TEXT}")"
RUNNER_POLICY_SHA="$(sha_file policies/seccomp/release-hermetic.policy)"

cargo run --locked -q -p jeryu-cli --bin jeryu-artifact-support -- \
  evidence \
  --evidence-dir "${EVIDENCE_DIR}" \
  --binary "${BINARY}" \
  --web-dist apps/web/dist \
  --route-probe "${ROUTE_PROBE}" \
  --commit "${COMMIT}" \
  --tree "${TREE}" \
  --repo "${REPO}" \
  --version "${VERSION}" \
  --manifest-sha "${MANIFEST_SHA}" \
  --split-lock-sha "${LOCK_SHA}" \
  --cargo-lock-sha "${CARGO_LOCK_SHA}" \
  --toolchain-sha "${TOOLCHAIN_SHA}" \
  --runner-policy-sha "${RUNNER_POLICY_SHA}"

cp repos.manifest.toml "${EVIDENCE_DIR}/repos.manifest.toml"
cp jeryu-split.lock.toml "${EVIDENCE_DIR}/jeryu-split.lock.toml"

rm -f "${BUNDLE}"
tar --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner \
  -czf "${BUNDLE}" -C "${EVIDENCE_DIR}" .
log "evidence bundle -> ${BUNDLE}"

SEED_VAR="JERYU_SIGNRAIL_ED25519_SEED"
if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  SEED_VAR="SIGNRAIL_ED25519_SEED"
fi

if [ -z "${ROLLBACK_TARGET}" ]; then
  require_strict "SIGNRAIL_ROLLBACK_TARGET or JERYU_RELEASE_ROLLBACK_TAG is required"
  exit 0
fi

if [ -z "${!SEED_VAR:-}" ]; then
  require_strict "${SEED_VAR} is required for SignRail release signing"
  exit 0
fi

[ -f "${SIGNRAIL_MANIFEST}" ] || die "missing jeryu-signrail manifest: ${SIGNRAIL_MANIFEST}"

rm -rf "${SIGNRAIL_DIR:?}/"*
mkdir -p "${SIGNRAIL_DIR}"
cargo run --manifest-path "${SIGNRAIL_MANIFEST}" --locked -q -p jeryu-signrail -- \
  sign-release \
  --artifact "${BUNDLE}" \
  --repo "${REPO}" \
  --sha "${COMMIT}" \
  --tree-sha "${TREE}" \
  --version "${VERSION}" \
  --rollback-target "${ROLLBACK_TARGET}" \
  --test-status "ci-and-artifact-support-passed" \
  --store-root "${SIGNRAIL_STORE_ROOT}" \
  --out-dir "${SIGNRAIL_DIR}" \
  --ci-ir-hash "sha256:${MANIFEST_SHA}" \
  --runner-rootfs-digest "sha256:${RUNNER_POLICY_SHA:-${MANIFEST_SHA}}" \
  --toolchain-digest "sha256:${TOOLCHAIN_SHA}" \
  --cargo-lock-digest "sha256:${CARGO_LOCK_SHA}" \
  > "${SIGNRAIL_DIR}/summary.json"

cargo run --locked -q -p jeryu-cli --bin jeryu-artifact-support -- \
  pubkey \
  --summary "${SIGNRAIL_DIR}/summary.json" \
  --out "${SIGNRAIL_DIR}/pubkey.hex"

cargo run --manifest-path "${SIGNRAIL_MANIFEST}" --locked -q -p jeryu-signrail -- \
  verify-release \
  --release "${SIGNRAIL_DIR}/release.json" \
  --stage prod \
  --store-root "${SIGNRAIL_STORE_ROOT}" \
  --pubkey-file "${SIGNRAIL_DIR}/pubkey.hex" \
  --json \
  > "${SIGNRAIL_DIR}/verify-prod.json"

log "SignRail evidence -> ${SIGNRAIL_DIR}"
