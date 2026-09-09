#!/usr/bin/env bash
# Contract test for scripts/emit-release-receipt.sh.
#
# The positive case is hermetic: it runs the real emitter against a mock bundle
# and a fake `git verify-commit` transcript. Negative cases prove the final
# receipt fails closed when signed-commit proof, rollback evidence, or PR
# publication metadata is missing.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EMITTER="${ROOT}/scripts/emit-release-receipt.sh"
JQ_BIN="$(command -v jq || true)"
[ -x "${EMITTER}" ] || { echo "FAIL: ${EMITTER} not executable"; exit 1; }
[ -n "${JQ_BIN}" ] || { echo "SKIP: jq not installed"; exit 0; }

PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); printf 'ok   - %s\n' "$1"; }
no() { FAIL=$((FAIL + 1)); printf 'FAIL - %s\n' "$1"; }
check() {
  if eval "$2"; then
    ok "$1"
  else
    no "$1"
  fi
}

WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

FAKE_COMMIT="0123456789abcdef0123456789abcdef01234567"
PREV_TAG="v0.0.0-prev"
PREV_BIN_SHA="1111111111111111111111111111111111111111111111111111111111111111"
PREV_SIG_SHA="2222222222222222222222222222222222222222222222222222222222222222"
PREV_CERT_SHA="3333333333333333333333333333333333333333333333333333333333333333"
SUPPORT_DIGEST="sha256:4444444444444444444444444444444444444444444444444444444444444444"

FAKE_BIN="${WORK}/bin"
mkdir -p "$FAKE_BIN"
cat >"${FAKE_BIN}/git" <<'SH'
#!/usr/bin/env bash
if [ "$1" = "verify-commit" ]; then
  if [ "${FAKE_GIT_VERIFY_OK:-1}" = "1" ]; then
    echo "[GNUPG:] GOODSIG fake-signer"
    exit 0
  fi
  echo "[GNUPG:] BADSIG fake-signer" >&2
  exit 1
fi
exit 1
SH
chmod +x "${FAKE_BIN}/git"
cat >"${FAKE_BIN}/gh" <<'SH'
#!/usr/bin/env bash
exit 1
SH
chmod +x "${FAKE_BIN}/gh"

make_bundle() {
  local bundle="$1" signrail="$2" publish="$3" support_bundle="$4"
  mkdir -p "$bundle" "$signrail/stage-receipts" "$(dirname "$publish")"
  printf 'mock-jeryu-binary\n' >"${bundle}/jeryu"
  jq -n '{spdx: "mock"}' >"${bundle}/sbom.spdx.json"
  jq -n '{cdx: "mock"}' >"${bundle}/sbom.cdx.json"
  jq -n --arg commit "$FAKE_COMMIT" '{
    "_type": "https://in-toto.io/Statement/v1",
    predicateType: "https://slsa.dev/provenance/v1",
    predicate: {
      buildDefinition: {
        externalParameters: {
          sourceCommit: $commit
        }
      }
    }
  }' >"${bundle}/provenance.json"
  printf 'cosign mock\n' >"${bundle}/cosign.txt"
  printf 'grype mock\n' >"${bundle}/grype-scan.json"
  printf 'artifact-support mock\n' >"$support_bundle"

  jq -n --arg commit "$FAKE_COMMIT" --arg prev "$PREV_TAG" '{
    commit_sha: $commit,
    rollback: {
      previous_release: $prev
    }
  }' >"${signrail}/release.json"
  jq -n '{sbom: "mock"}' >"${signrail}/sbom.json"
  jq -n '[{statement: "mock"}]' >"${signrail}/provenance.json"
  jq -n '{signature_coverage_percent: 100}' >"${signrail}/witness.json"
  jq -n '{summary: "mock"}' >"${signrail}/summary.json"
  for stage in local dev-canary prod; do
    jq -n \
      --arg stage "$stage" \
      --arg commit "$FAKE_COMMIT" \
      --arg prev "$PREV_TAG" \
      --arg artifact "$SUPPORT_DIGEST" \
      '{
        kind: "signrail-stage",
        payload: {
          stage: $stage,
          sha: $commit,
          rollback_target: $prev,
          artifact_digest: $artifact,
          signature_coverage_percent: 100
        }
      }' >"${signrail}/stage-receipts/${stage}.json"
  done

  jq -n --arg commit "$FAKE_COMMIT" '{
    schema: "jeryu.ci-fast-push.publish/v1",
    mode: "pr",
    branch: "release/v0.0.0-test",
    base: "main",
    commit: $commit,
    pr: {
      url: "https://github.com/neverhuman/jeryu/pull/1",
      number: "1"
    }
  }' >"$publish"
}

run_emitter() {
  local bundle="$1" signrail="$2" publish="$3" support_bundle="$4" receipt="$5"
  PATH="${FAKE_BIN}:$PATH" \
  FAKE_GIT_VERIFY_OK="${FAKE_GIT_VERIFY_OK:-1}" \
  JERYU_RELEASE_COMMIT="$FAKE_COMMIT" \
  JERYU_RELEASE_TAG="v0.0.0-test" \
  GITHUB_REPOSITORY="neverhuman/jeryu" \
  JERYU_RELEASE_INITIAL_DEPLOY=0 \
  JERYU_RELEASE_ROLLBACK_TAG="$PREV_TAG" \
  JERYU_RELEASE_ROLLBACK_SHA256="$PREV_BIN_SHA" \
  JERYU_RELEASE_ROLLBACK_SIGNATURE_SHA256="$PREV_SIG_SHA" \
  JERYU_RELEASE_ROLLBACK_CERTIFICATE_SHA256="$PREV_CERT_SHA" \
  JERYU_RELEASE_PUBLICATION_FILE="$publish" \
  JERYU_RELEASE_SIGNRAIL_DIR="$signrail" \
  JERYU_RELEASE_ARTIFACT_SUPPORT_BUNDLE="$support_bundle" \
    bash "$EMITTER" "$bundle" >"$receipt"
}

run_initial_emitter() {
  local bundle="$1" signrail="$2" publish="$3" support_bundle="$4" receipt="$5" marker="$6"
  PATH="${FAKE_BIN}:$PATH" \
  FAKE_GIT_VERIFY_OK="${FAKE_GIT_VERIFY_OK:-1}" \
  JERYU_RELEASE_COMMIT="$FAKE_COMMIT" \
  JERYU_RELEASE_TAG="v0.0.0-test" \
  GITHUB_REPOSITORY="jeryu/jeryu-deploy" \
  JERYU_RELEASE_INITIAL_DEPLOY=1 \
  JERYU_RELEASE_ROLLBACK_TAG="$marker" \
  JERYU_RELEASE_PUBLICATION_FILE="$publish" \
  JERYU_RELEASE_SIGNRAIL_DIR="$signrail" \
  JERYU_RELEASE_ARTIFACT_SUPPORT_BUNDLE="$support_bundle" \
    bash "$EMITTER" "$bundle" >"$receipt"
}

BUNDLE="${WORK}/bundle"
SIGNRAIL="${BUNDLE}/artifact-support-signrail"
PUBLISH="${WORK}/publish.json"
SUPPORT_BUNDLE="${BUNDLE}/artifact-support-evidence.tar.gz"
RECEIPT="${BUNDLE}/release-receipt.json"
make_bundle "$BUNDLE" "$SIGNRAIL" "$PUBLISH" "$SUPPORT_BUNDLE"
run_emitter "$BUNDLE" "$SIGNRAIL" "$PUBLISH" "$SUPPORT_BUNDLE" "$RECEIPT"

check "receipt is valid JSON" "jq -e . '${RECEIPT}' >/dev/null"
check "receipt schema is v2" "jq -e '.schema==\"jeryu.release-receipt/v2\"' '${RECEIPT}' >/dev/null"
check "receipt names the candidate commit" "jq -e --arg commit '${FAKE_COMMIT}' '.commit==\$commit' '${RECEIPT}' >/dev/null"
check "signed commit proof is recorded" "jq -e '(.signed_commit.verified==true) and (.artifacts.\"signed-commit.txt\".sha256|test(\"^[0-9a-f]{64}$\"))' '${RECEIPT}' >/dev/null"
check "PR publication metadata is recorded" "jq -e '.publication.mode==\"pr\" and .publication.branch==\"release/v0.0.0-test\" and .publication.pr.number==\"1\"' '${RECEIPT}' >/dev/null"
check "rollback evidence names previous signed artifact" "jq -e --arg prev '${PREV_TAG}' --arg sha '${PREV_BIN_SHA}' '(.rollback.previous_release==\$prev) and (.rollback.previous_binary_sha256==\$sha)' '${RECEIPT}' >/dev/null"
check "artifact-support SignRail evidence is recorded" "jq -e '(.artifact_support.files.\"stage-receipts/prod.json\".sha256|test(\"^[0-9a-f]{64}$\"))' '${RECEIPT}' >/dev/null"
check "checksum manifest written" "[ -s '${BUNDLE}/SHA256SUMS' ]"
CHECKSUM_ACTUAL="$(sha256sum "${BUNDLE}/SHA256SUMS" | awk '{print $1}')"
CHECKSUM_RECEIPT="$(jq -r '.checksum_manifest.sha256' "$RECEIPT")"
check "receipt checksum manifest digest matches" "[ '${CHECKSUM_ACTUAL}' = '${CHECKSUM_RECEIPT}' ]"
check "rollback.json written + valid JSON" "jq -e . '${BUNDLE}/rollback.json' >/dev/null"

INITIAL_MARKER="atomicsoul-initial-install"
INITIAL="${WORK}/initial"
make_bundle "$INITIAL/bundle" "$INITIAL/bundle/artifact-support-signrail" "$INITIAL/publish.json" "$INITIAL/bundle/artifact-support-evidence.tar.gz"
for file in "$INITIAL/bundle/artifact-support-signrail/release.json" "$INITIAL/bundle/artifact-support-signrail/stage-receipts/"*.json; do
  tmp="${file}.tmp"
  jq --arg marker "$INITIAL_MARKER" \
    '(.rollback.previous_release? // empty) |= $marker
     | (.payload.rollback_target? // empty) |= $marker
     | (.payload.artifact_digest? // empty) |= sub("^sha256:"; "")' \
    "$file" >"$tmp"
  mv "$tmp" "$file"
done
run_initial_emitter \
  "$INITIAL/bundle" \
  "$INITIAL/bundle/artifact-support-signrail" \
  "$INITIAL/publish.json" \
  "$INITIAL/bundle/artifact-support-evidence.tar.gz" \
  "$INITIAL/bundle/release-receipt.json" \
  "$INITIAL_MARKER"
check "initial deploy receipt avoids previous artifact digests" "jq -e --arg marker '${INITIAL_MARKER}' '.rollback.initial_deploy==true and .rollback.previous_release==\$marker and (.rollback.previous_binary_sha256|not)' '${INITIAL}/bundle/release-receipt.json' >/dev/null"
check "raw SignRail artifact digest is normalized" "jq -e --arg digest '${SUPPORT_DIGEST}' '.artifact_support.signed_artifact_digest==\$digest' '${INITIAL}/bundle/release-receipt.json' >/dev/null"
check "initial deploy rollback.json is explicit" "jq -e '.initial_deploy==true and (.rollback_command|contains(\"disable --now jeryu.service\"))' '${INITIAL}/bundle/rollback.json' >/dev/null"

NEG="${WORK}/neg-unsigned"
make_bundle "$NEG/bundle" "$NEG/bundle/artifact-support-signrail" "$NEG/publish.json" "$NEG/bundle/artifact-support-evidence.tar.gz"
if FAKE_GIT_VERIFY_OK=0 run_emitter "$NEG/bundle" "$NEG/bundle/artifact-support-signrail" "$NEG/publish.json" "$NEG/bundle/artifact-support-evidence.tar.gz" "$NEG/bundle/release-receipt.json" >/dev/null 2>&1; then
  no "missing signed-commit proof is rejected"
else
  ok "missing signed-commit proof is rejected"
fi

NEG="${WORK}/neg-rollback"
make_bundle "$NEG/bundle" "$NEG/bundle/artifact-support-signrail" "$NEG/publish.json" "$NEG/bundle/artifact-support-evidence.tar.gz"
if PATH="${FAKE_BIN}:$PATH" \
  JERYU_RELEASE_COMMIT="$FAKE_COMMIT" \
  JERYU_RELEASE_TAG="v0.0.0-test" \
  JERYU_RELEASE_INITIAL_DEPLOY=0 \
  JERYU_RELEASE_PUBLICATION_FILE="$NEG/publish.json" \
  JERYU_RELEASE_SIGNRAIL_DIR="$NEG/bundle/artifact-support-signrail" \
  JERYU_RELEASE_ARTIFACT_SUPPORT_BUNDLE="$NEG/bundle/artifact-support-evidence.tar.gz" \
    bash "$EMITTER" "$NEG/bundle" >"$NEG/bundle/release-receipt.json" 2>/dev/null; then
  no "missing rollback evidence is rejected"
else
  ok "missing rollback evidence is rejected"
fi

NEG="${WORK}/neg-publication"
make_bundle "$NEG/bundle" "$NEG/bundle/artifact-support-signrail" "$NEG/publish.json" "$NEG/bundle/artifact-support-evidence.tar.gz"
jq -n --arg commit "$FAKE_COMMIT" '{
  schema: "jeryu.ci-fast-push.publish/v1",
  mode: "pr",
  branch: "release/v0.0.0-test",
  base: "main",
  commit: $commit,
  pr: {}
}' >"$NEG/publish.json"
if run_emitter "$NEG/bundle" "$NEG/bundle/artifact-support-signrail" "$NEG/publish.json" "$NEG/bundle/artifact-support-evidence.tar.gz" "$NEG/bundle/release-receipt.json" >/dev/null 2>&1; then
  no "missing branch/PR publication metadata is rejected"
else
  ok "missing branch/PR publication metadata is rejected"
fi

printf '\n[test-emit-release-receipt] %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
