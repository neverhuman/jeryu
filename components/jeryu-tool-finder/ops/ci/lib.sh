#!/usr/bin/env bash
set -euo pipefail

# BEGIN GENERATED JANKURAI PIN — DO NOT EDIT
export JERYU_JANKURAI_SOURCE_REPO="https://github.com/neverhuman/jankurai.git"
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

# Resolve the family-pinned auditor the same way the authoritative Layer-2
# scoring funnel does (jeryu-deploy ops/ci/common.sh): JERYU_JANKURAI_BIN,
# defaulting to the jeryu-managed global install. PATH is the last resort —
# stray jankurai builds carry drifted heuristics and inconsistent caps.
JANKURAI_BIN="${JERYU_JANKURAI_BIN:-$HOME/.jeryu/bin/jankurai}"
if [[ ! -x "$JANKURAI_BIN" ]]; then
  JANKURAI_BIN="jankurai"
fi

require_tool() {
  local name="$1"
  command -v "$name" >/dev/null 2>&1 || {
    printf 'missing required tool: %s\n' "$name" >&2
    exit 1
  }
}

require_jankurai() {
  if [[ "${JERYU_MONOREPO_CANDIDATE:-0}" != "0" ]]; then
    local candidate_root
    candidate_root="$(env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null \
      GIT_CONFIG_NOSYSTEM=1 /usr/bin/git -C "$(dirname -- "${BASH_SOURCE[0]}")" \
      rev-parse --show-toplevel)" || return 1
    # shellcheck source=/dev/null
    source "${candidate_root}/components/jeryu-tool/ops/verify-public-candidate.sh"
    require_public_candidate_jankurai
    return
  fi
  local mode=receipt-bound
  local expected_broker="/opt/jain-ci/authority/release-bin/jankurai"
  local expected_governed="/home/ubuntu/.jeryu/bin/jankurai"
  local bin bin_dir governed_root normalized resolved actual actual_sha receipt receipt_digest receipt_sha
  local expected_test=false expected_verification=release-authoritative
  local expected_governance=governed expected_protected=true
  local expected_protection=immutable-main-v1 found_receipt=0
  local -a receipt_candidates=()
  if [[ "${JAIN_RELEASE_CI:-0}" == "1" ]]; then
    mode=release-broker
    resolved="$(type -P -- jankurai 2>/dev/null || true)"
    if [[ "${resolved}" != "${expected_broker}" ]]; then
      printf 'release broker Jankurai path mismatch: expected %s, resolved %s\n' \
        "${expected_broker}" "${resolved:-missing}" >&2
      exit 1
    fi
    bin="${resolved}"
  else
    bin="${JERYU_GOVERNED_JANKURAI_BIN:-${expected_governed}}"
  fi
  if [[ "${bin}" != /* || ! -f "${bin}" || -L "${bin}" || ! -x "${bin}" ]]; then
    printf 'governed jankurai must be an absolute executable regular file: %s\n' "${bin}" >&2
    exit 1
  fi
  normalized="$(realpath -m "${bin}")"
  if [[ "${normalized}" != "${bin}" ]]; then
    printf 'governed jankurai path traverses a symlink: %s -> %s\n' "${bin}" "${normalized}" >&2
    exit 1
  fi
  if [[ "${mode}" == "release-broker" &&
        "$(stat -c '%a:%h' -- "${bin}" 2>/dev/null || true)" != "555:1" ]]; then
    printf 'release broker Jankurai custody mismatch: expected mode 0555 and one link at %s\n' \
      "${bin}" >&2
    exit 1
  elif [[ "${mode}" != "release-broker" &&
          "$(stat -c '%h' -- "${bin}" 2>/dev/null || true)" != "1" ]]; then
    printf 'governed jankurai custody mismatch: expected one link at %s\n' \
      "${bin}" >&2
    exit 1
  fi
  if [[ "${mode}" != "release-broker" ]]; then
    bin_dir="$(dirname "${bin}")"
    export PATH="${bin_dir}:${PATH}"
    resolved="$(type -P -- jankurai 2>/dev/null || true)"
    if [[ "${resolved}" != "${bin}" ]]; then
      printf 'governed jankurai shadowed: expected %s, resolved %s\n' \
        "${bin}" "${resolved:-missing}" >&2
      exit 1
    fi
  fi
  actual="$("${bin}" --version 2>/dev/null || true)"
  actual_sha="$(sha256sum "${bin}" 2>/dev/null | awk '{print $1}')"
  if [[ "${actual}" != "${JERYU_JANKURAI_VERSION}" ]] ||
     [[ "${actual_sha}" != "${JERYU_JANKURAI_SHA256}" ]]; then
    printf 'governed jankurai identity mismatch at %s: version=%s sha256=%s\n' \
      "${bin}" "${actual:-missing}" "${actual_sha:-missing}" >&2
    exit 1
  fi
  export JERYU_GOVERNED_JANKURAI_BIN="${bin}"
  if [[ "${mode}" == "release-broker" ]]; then
    if [[ -n "${JERYU_JANKURAI_RECEIPT:-}" ||
          -n "${JERYU_JANKURAI_RECEIPT_SHA256:-}" ||
          "${JERYU_JANKURAI_ALLOW_TEST_RECEIPT:-0}" != "0" ]]; then
      printf 'release broker Jankurai rejects caller receipt authority\n' >&2
      exit 1
    fi
    found_receipt=1
  elif [[ "${JERYU_JANKURAI_ALLOW_TEST_RECEIPT:-0}" == "1" ]]; then
    expected_test=true
    expected_verification=diagnostic-candidate
    expected_governance=diagnostic-candidate
    expected_protected=false
    expected_protection=not-applicable
  fi
  if [[ "${mode}" == "release-broker" ]]; then
    receipt_candidates=()
  elif [[ -n "${JERYU_JANKURAI_RECEIPT:-}" ]]; then
    receipt_candidates=("${JERYU_JANKURAI_RECEIPT}")
  elif [[ "${bin}" == "${expected_governed}" ]]; then
    governed_root="$(dirname "$(dirname "${expected_governed}")")"
    receipt_candidates=("${governed_root}"/receipts/jankurai/sha256/*.json)
  else
    printf 'non-governed jankurai requires an explicit installation receipt: %s\n' \
      "${bin}" >&2
    exit 1
  fi
  for receipt in "${receipt_candidates[@]}"; do
    [[ -f "${receipt}" ]] || continue
    receipt_digest="$(basename "${receipt}" .json)"
    [[ "${receipt_digest}" =~ ^[0-9a-f]{64}$ ]] || continue
    receipt_sha="$(sha256sum "${receipt}" | awk '{print $1}')"
    [[ "${receipt_sha}" == "${receipt_digest}" ]] || continue
    if jq -e \
      --arg remote "${JERYU_JANKURAI_SOURCE_REPO}" \
      --arg commit "${JERYU_JANKURAI_SOURCE_REV}" \
      --arg tag "${JERYU_JANKURAI_SOURCE_TAG}" \
      --arg tree "${JERYU_JANKURAI_SOURCE_TREE}" \
      --arg archive "${JERYU_JANKURAI_SOURCE_ARCHIVE_SHA256}" \
      --arg lock "${JERYU_JANKURAI_CARGO_LOCK_SHA256}" \
      --arg rustc "${JERYU_JANKURAI_RUSTC_VERSION}" \
      --arg cargo "${JERYU_JANKURAI_CARGO_VERSION}" \
      --arg triple "${JERYU_JANKURAI_TARGET_TRIPLE}" \
      --arg mode "${JERYU_JANKURAI_BUILD_MODE}" \
      --arg package_path "${JERYU_JANKURAI_PACKAGE_PATH}" \
      --arg builder_image "${JERYU_JANKURAI_BUILDER_IMAGE}" \
      --arg builder_image_id "${JERYU_JANKURAI_BUILDER_IMAGE_ID}" \
      --arg linker "${JERYU_JANKURAI_LINKER_VERSION}" \
      --arg glibc "${JERYU_JANKURAI_GLIBC_VERSION}" \
      --arg vendor "${JERYU_JANKURAI_VENDOR_FILES_SHA256}" \
      --arg vendor_count "${JERYU_JANKURAI_VENDOR_FILE_COUNT}" \
      --arg cargo_config "${JERYU_JANKURAI_CARGO_CONFIG_SHA256}" \
      --arg environment "${JERYU_JANKURAI_BUILD_ENVIRONMENT}" \
      --arg rustflags "${JERYU_JANKURAI_RUSTFLAGS}" \
      --arg command "${JERYU_JANKURAI_BUILD_COMMAND}" \
      --arg context "${JERYU_JANKURAI_BUILD_CONTEXT_SHA256}" \
      --arg digest "${JERYU_JANKURAI_SHA256}" \
      --arg version "${JERYU_JANKURAI_VERSION}" \
      --arg path "${bin}" \
      --arg verification "${expected_verification}" \
      --arg governance "${expected_governance}" \
      --arg protection "${expected_protection}" \
      --argjson protected_main "${expected_protected}" \
      --argjson test_mode "${expected_test}" \
      '.schema == "jeryu.jankurai-installation/v2" and
       .source.remote == $remote and .source.commit == $commit and .source.tag == $tag and
       .source.tree == $tree and .source.archive_sha256 == $archive and
       .source.cargo_lock_sha256 == $lock and .source.verification == $verification and
       .build.rustc == $rustc and .build.cargo == $cargo and
       .build.target_triple == $triple and .build.mode == $mode and
       .build.package_path == $package_path and
       .build.builder_image == $builder_image and
       .build.builder_image_id == $builder_image_id and
       .build.linker == $linker and .build.glibc == $glibc and
       .build.vendor_files_sha256 == $vendor and
       .build.vendor_file_count == $vendor_count and
       .build.cargo_config_sha256 == $cargo_config and
       .build.environment == $environment and .build.rustflags == $rustflags and
       .build.command == $command and .build.context_sha256 == $context and
       .build.cargo_net_offline == true and .build.closed_vendor == true and
       .build.network_none == true and .build.read_only_root == true and
       .build.non_root == true and .build.capabilities_dropped == true and
       .build.no_new_privileges == true and
       .build.container_engine_path == "/usr/bin/docker" and
       .build.git_global_config_disabled == true and .build.git_system_config_disabled == true and
       .build.git_http_follow_redirects == false and .build.git_terminal_prompt == false and
       .build.jankurai_update_check == false and
       .build.network_scope ==
         "local-forge-source-plus-closed-vendor-network-none" and
       .build.no_proxy == "127.0.0.1,localhost,::1" and
       .governance.status == $governance and
       .governance.manifest_repo ==
         "http://127.0.0.1:8787/git/jeryu/jeryu-tool.git" and
       (.governance.manifest_commit | test("^[0-9a-f]{40}$")) and
       (.governance.manifest_tree | test("^[0-9a-f]{40}$")) and
       (.governance.manifest_sha256 | test("^[0-9a-f]{64}$")) and
       .governance.protected_main == $protected_main and
       .governance.protection_policy == $protection and
       .binary.sha256 == $digest and .binary.version_output == $version and
       .installation.path == $path and .installation.atomic == true and
       .test_mode == $test_mode and .conclusion == "success"' "${receipt}" >/dev/null; then
      export JERYU_JANKURAI_RECEIPT="${receipt}"
      export JERYU_JANKURAI_RECEIPT_SHA256="${receipt_digest}"
      found_receipt=1
      break
    fi
  done
  if [[ "${mode}" != "release-broker" && "${found_receipt}" -ne 1 ]]; then
    printf 'governed jankurai receipt mismatch: binary=%s test_mode=%s\n' \
      "${bin}" "${expected_test}" >&2
    exit 1
  fi
  export JANKURAI_NO_UPDATE_CHECK=1 GIT_TERMINAL_PROMPT=0
}

jankurai() {
  require_jankurai || return 1
  if [[ "${JERYU_MONOREPO_CANDIDATE:-0}" == "1" ]]; then
    command "${JERYU_CANDIDATE_JANKURAI_DESCRIPTOR:?candidate descriptor is missing}" "$@"
    return
  fi
  command "${JERYU_GOVERNED_JANKURAI_BIN}" "$@"
}
