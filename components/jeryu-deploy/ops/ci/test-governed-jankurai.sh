#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GOVERNED="${JERYU_GOVERNED_JANKURAI_BIN:-/home/ubuntu/.jeryu/bin/jankurai}"
TEST_ROOT="$(mktemp -d)"

cleanup() {
  rm -rf -- "${TEST_ROOT}"
}
trap cleanup EXIT

verify_at() {
  JERYU_GOVERNED_JANKURAI_BIN="$1" bash -c \
    'set -euo pipefail; source "$1/ops/ci/lib.sh"; require_jankurai' _ "${ROOT}"
}

expect_rejected() {
  local label="$1" path="$2"
  if verify_at "${path}" >/dev/null 2>&1; then
    printf 'hostile jankurai fixture was accepted: %s (%s)\n' "${label}" "${path}" >&2
    exit 1
  fi
}

# The governed host binary must also carry a digest-bound installation receipt.
verify_at "${GOVERNED}"

# The networkless agent image carries a distinct path-bound copy of the same
# authority. Keep its static receipt content-addressed and prove that the image
# and smoke test consume the exact rendered identity, not a prior pin.
source "${ROOT}/ops/ci/lib.sh"
image_receipt="${ROOT}/images/agent-sandbox/jankurai-installation-receipt.json"
image_receipt_sha="$(sha256sum "${image_receipt}" | awk '{print $1}')"
image_receipt_path="/opt/jeryu/receipts/jankurai/sha256/${image_receipt_sha}.json"
[[ "$(jq -r '.binary.sha256' "${image_receipt}")" == "${JERYU_JANKURAI_SHA256}" ]]
[[ "$(jq -r '.binary.version_output' "${image_receipt}")" == "${JERYU_JANKURAI_VERSION}" ]]
[[ "$(jq -r '.source.remote' "${image_receipt}")" == "${JERYU_JANKURAI_SOURCE_REPO}" ]]
[[ "$(jq -r '.source.tag' "${image_receipt}")" == "${JERYU_JANKURAI_SOURCE_TAG}" ]]
[[ "$(jq -r '.source.commit' "${image_receipt}")" == "${JERYU_JANKURAI_SOURCE_REV}" ]]
[[ "$(jq -r '.source.tree' "${image_receipt}")" == "${JERYU_JANKURAI_SOURCE_TREE}" ]]
[[ "$(jq -r '.source.archive_sha256' "${image_receipt}")" == "${JERYU_JANKURAI_SOURCE_ARCHIVE_SHA256}" ]]
[[ "$(jq -r '.source.cargo_lock_sha256' "${image_receipt}")" == "${JERYU_JANKURAI_CARGO_LOCK_SHA256}" ]]
[[ "$(jq -r '.governance.manifest_commit' "${image_receipt}")" == "e8218f9f39bf38277646f31daf2f2ee56e9a7eff" ]]
[[ "$(jq -r '.governance.manifest_tree' "${image_receipt}")" == "dae370c78d9e71123bf239ef06add4ee4d04e34e" ]]
[[ "$(jq -r '.governance.manifest_sha256' "${image_receipt}")" == "be001dc52c66da5669167f3e429d882184931baa3d7a0e53b605c17425872b5a" ]]
[[ "$(jq -r '.installation.path' "${image_receipt}")" == "/opt/rust/cargo/bin/jankurai" ]]
grep -Fq -- "${image_receipt_path}" "${ROOT}/images/agent-sandbox/Dockerfile"
grep -Fq -- "${image_receipt_path}" "${ROOT}/ops/agent-sandbox/smoke.sh"

expect_rejected missing "${TEST_ROOT}/missing"

ln -s "${GOVERNED}" "${TEST_ROOT}/symlink"
expect_rejected symlink "${TEST_ROOT}/symlink"

printf '#!/usr/bin/env bash\nprintf "jankurai 1.6.10\\n"\n' > "${TEST_ROOT}/wrong-version"
chmod 0755 "${TEST_ROOT}/wrong-version"
expect_rejected wrong-version "${TEST_ROOT}/wrong-version"

printf '#!/usr/bin/env bash\nprintf "jankurai 1.6.11\\n"\n' > "${TEST_ROOT}/same-version-substitute"
chmod 0755 "${TEST_ROOT}/same-version-substitute"
expect_rejected same-version-wrong-digest "${TEST_ROOT}/same-version-substitute"

# A hostile ambient PATH is neutralized deterministically: the verifier prepends
# the governed binary directory, then proves the resulting resolution and receipt.
mkdir "${TEST_ROOT}/hostile-path"
cp -- "${TEST_ROOT}/same-version-substitute" "${TEST_ROOT}/hostile-path/jankurai"
before="$(PATH="${TEST_ROOT}/hostile-path:${PATH}" command -v jankurai)"
[[ "${before}" == "${TEST_ROOT}/hostile-path/jankurai" ]]
after="$(JERYU_GOVERNED_JANKURAI_BIN="${GOVERNED}" PATH="${TEST_ROOT}/hostile-path:${PATH}" \
  bash -c 'set -euo pipefail; source "$1/ops/ci/lib.sh"; require_jankurai; command -v jankurai' \
  _ "${ROOT}")"
[[ "${after}" == "${GOVERNED}" ]]

printf 'governed jankurai hostile identity and PATH neutralization tests ok\n'
