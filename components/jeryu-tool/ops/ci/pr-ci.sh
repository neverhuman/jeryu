#!/usr/bin/env bash
# Canonical release-authoritative local PR gate for jeryu-tool. split-host-ci
# posts `jeryu-tool/required` from this script. The GitHub workflow is an
# explicitly non-authoritative static mirror because it cannot reach local Jeryu.
set -euo pipefail

# BEGIN GENERATED JANKURAI PIN — DO NOT EDIT
export JERYU_JANKURAI_SOURCE_REPO="http://127.0.0.1:8787/git/jeryu/jankurai.git"
export JERYU_JANKURAI_VERSION="jankurai 1.6.11"
export JERYU_JANKURAI_SHA256="9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c"
export JERYU_JANKURAI_SOURCE_REV="b88562fdb124aa86dedd70ab972e7d0d87e58be1"
export JERYU_JANKURAI_SOURCE_TAG="v1.6.11-deadlang-precision-split.3"
export JERYU_JANKURAI_SOURCE_TREE="611229e54938c0e8808896e369fd54d095d258f7"
export JERYU_JANKURAI_SOURCE_ARCHIVE_SHA256="903a231eca8f6a1f050953b603d5a278a1606abcdf47434eb1b45262d74068aa"
export JERYU_JANKURAI_CARGO_LOCK_SHA256="b9acb981c326226a687d0b6703e4f7ee303148e9e1a6dda1aa03d77988820f6a"
export JERYU_JANKURAI_RUST_TOOLCHAIN="1.95.0"
export JERYU_JANKURAI_RUSTC_VERSION="rustc 1.95.0 (59807616e 2026-04-14)"
export JERYU_JANKURAI_CARGO_VERSION="cargo 1.95.0 (f2d3ce0bd 2026-03-21)"
export JERYU_JANKURAI_TARGET_TRIPLE="x86_64-unknown-linux-gnu"
export JERYU_JANKURAI_BUILD_MODE="oci-vendor-locked-offline-workspace-member-v2"
export JERYU_JANKURAI_PACKAGE_PATH="crates/jankurai"
export JERYU_JANKURAI_BUILDER_IMAGE="rust@sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082"
export JERYU_JANKURAI_BUILDER_IMAGE_ID="sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082"
export JERYU_JANKURAI_LINKER_VERSION="GNU ld (GNU Binutils for Debian) 2.40"
export JERYU_JANKURAI_GLIBC_VERSION="ldd (Debian GLIBC 2.36-9+deb12u14) 2.36"
export JERYU_JANKURAI_VENDOR_FILES_SHA256="a7e332f4495d9748ea020ae8ee37c4240f0f035059799bd3dc74497437143d99"
export JERYU_JANKURAI_VENDOR_FILE_COUNT="14889"
export JERYU_JANKURAI_CARGO_CONFIG_SHA256="b8982c761d62e447f2d1653c199d2d58e6b2de6c5a6f8ddba3d38e47b7f863d6"
export JERYU_JANKURAI_BUILD_ENVIRONMENT="CARGO_NET_OFFLINE=true,HOME=/tmp,LANG=C,LC_ALL=C,SOURCE_DATE_EPOCH=0,TZ=UTC"
export JERYU_JANKURAI_RUSTFLAGS="--remap-path-prefix=/opt/jeryu/jankurai=/jankurai-build/source --remap-path-prefix=/opt/jeryu/vendor=/jankurai-build/vendor --remap-path-prefix=/opt/jeryu/target=/jankurai-build/target --remap-path-prefix=/usr/local/cargo=/jankurai-build/cargo"
export JERYU_JANKURAI_BUILD_COMMAND="cargo install --locked --offline --path /opt/jeryu/jankurai/crates/jankurai --root /opt/jeryu/out --bin jankurai"
export JERYU_JANKURAI_BUILD_CONTEXT_SHA256="889d19f86fc390b0f0cf0bd6ecb4d451c51a2d6fb328e5520e4310e7ee5dedd6"
# END GENERATED JANKURAI PIN

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

# Release-full CI accepts only the root broker's fixed, PATH-selected auditor.
# A local premerge manifest PR may instead qualify its exact pinned candidate in
# /tmp, with a content-addressed diagnostic receipt, without granting release
# authority or mutating an installed auditor.
unset JERYU_GOVERNED_JANKURAI_BIN JERYU_JANKURAI_BIN JERYU_JANKURAI_RECEIPT \
  JERYU_JANKURAI_RECEIPT_SHA256 JERYU_JANKURAI_ALLOW_TEST_RECEIPT
host_bin="$(command -v jankurai 2>/dev/null || true)"
host_version=""
host_sha=""
if [[ -f "${host_bin}" && ! -L "${host_bin}" ]]; then
  host_version="$("${host_bin}" --version 2>/dev/null || true)"
  host_sha="$(sha256sum "${host_bin}" 2>/dev/null | awk '{print $1}' || true)"
fi
candidate_root=""
if [[ "${JAIN_RELEASE_CI:-0}" == "1" ]]; then
  source ops/ci/lib.sh
  require_jankurai
  qualification_mode="release-broker"
elif [[ "${host_version}" == "${JERYU_JANKURAI_VERSION}" &&
        "${host_sha}" == "${JERYU_JANKURAI_SHA256}" ]]; then
  source ops/ci/lib.sh
  require_jankurai
  qualification_mode="receipt-bound-host"
else
  if [[ "${JERYU_TOOL_REQUIRE_GOVERNED_HOST:-0}" == "1" ]]; then
    printf 'governed-host Jankurai required: version=%s sha256=%s\n' \
      "${host_version:-missing}" "${host_sha:-missing}" >&2
    exit 1
  fi
  candidate_root="$(mktemp -d /tmp/jeryu-tool-premerge-candidate.XXXXXX)"
  trap 'rm -rf "${candidate_root}"' EXIT
  evidence_dir="${repo_root}/target/jankurai/premerge-candidate"
  rm -rf "${evidence_dir}"
  mkdir -p "${evidence_dir}"
  "${repo_root}/ops/qualify-jankurai-candidate.sh" "${candidate_root}" "${evidence_dir}"
  mapfile -t candidate_envs < <(find "${evidence_dir}" -maxdepth 1 -type f -name '*.env' -print)
  [[ "${#candidate_envs[@]}" -eq 1 ]] || {
    printf 'expected exactly one candidate qualification environment\n' >&2
    exit 1
  }
  # shellcheck source=/dev/null
  source "${candidate_envs[0]}"
  candidate_bin="${JERYU_JANKURAI_BIN:?candidate qualification did not select Jankurai}"
  candidate_bin_dir="$(dirname "${candidate_bin}")"
  export PATH="${candidate_bin_dir}:${PATH}"
  export JERYU_GOVERNED_JANKURAI_BIN="${candidate_bin}"
  source ops/ci/lib.sh
  require_jankurai
  qualification_mode="premerge-candidate"
fi
printf '[pr-ci] jankurai mode=%s bin=%s receipt=%s receipt_sha256=%s\n' \
  "${qualification_mode}" "${JERYU_GOVERNED_JANKURAI_BIN}" \
  "${JERYU_JANKURAI_RECEIPT:-not-product-visible}" \
  "${JERYU_JANKURAI_RECEIPT_SHA256:-not-product-visible}" >&2

# The manifest PR proves its own generated consumers first. After each protected
# consumer lands, the release lane runs the unscoped family check over canonical mains.
echo "[pr-ci] jankurai pin drift check (manifest-owner self scope)" >&2
bash ops/render-tool-manifest.sh --check --repo jeryu-tool

echo "[pr-ci] standard lanes" >&2
bash ops/ci/fast.sh
bash ops/ci/check.sh
bash ops/ci/score.sh
bash ops/ci/security.sh
bash ops/ci/artifact_support.sh
echo "[pr-ci] jeryu-tool OK" >&2
