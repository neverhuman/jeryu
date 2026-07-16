#!/usr/bin/env bash
# GENERATED Jankurai verifier. Installation is owned only by jeryu-tool.
set -euo pipefail

# BEGIN GENERATED JANKURAI PIN — DO NOT EDIT
export JERYU_GOVERNED_JANKURAI_BIN="${JERYU_JANKURAI_BIN:-/home/ubuntu/.jeryu/bin/jankurai}"
export JERYU_JANKURAI_SOURCE_REPO="http://127.0.0.1:8787/git/jeryu/jankurai.git"
export JERYU_JANKURAI_VERSION="jankurai 1.6.11"
export JERYU_JANKURAI_SHA256="fdb42e5fa7d9851c0729e59bf1e582c895aa9cfc03a7175b420c6025d2fd014e"
export JERYU_JANKURAI_SOURCE_REV="dface7397fe24d46b0b1885ddd5782c34edbff49"
export JERYU_JANKURAI_SOURCE_TAG="v1.6.11-deadlang-precision-split.1"
export JERYU_JANKURAI_SOURCE_TREE="34a8a1fb59bc4ebfadf12c45d95f169d06acc781"
export JERYU_JANKURAI_SOURCE_ARCHIVE_SHA256="2fbca5d04083e3c8d32f383d5b6b4520b8911690b26968c6fbcb210e1202b938"
export JERYU_JANKURAI_CARGO_LOCK_SHA256="b9acb981c326226a687d0b6703e4f7ee303148e9e1a6dda1aa03d77988820f6a"
export JERYU_JANKURAI_RUST_TOOLCHAIN="1.95.0"
export JERYU_JANKURAI_RUSTC_VERSION="rustc 1.95.0 (59807616e 2026-04-14)"
export JERYU_JANKURAI_CARGO_VERSION="cargo 1.95.0 (f2d3ce0bd 2026-03-21)"
export JERYU_JANKURAI_TARGET_TRIPLE="x86_64-unknown-linux-gnu"
export JERYU_JANKURAI_BUILD_MODE="cargo-install-locked-offline-path-v1"
# END GENERATED JANKURAI PIN

require_jankurai() {
  local bin="${JERYU_GOVERNED_JANKURAI_BIN}"
  local bin_dir normalized resolved actual actual_sha receipt receipt_digest receipt_sha
  local expected_test=false expected_verification=release-authoritative
  local expected_governance=governed expected_protected=true
  local expected_protection=immutable-main-v1 found_receipt=0
  local -a receipt_candidates=()
  if [[ "${bin}" != /* || ! -f "${bin}" || -L "${bin}" || ! -x "${bin}" ]]; then
    printf 'governed jankurai must be an absolute executable regular file: %s\n' "${bin}" >&2
    exit 1
  fi
  normalized="$(realpath -m "${bin}")"
  if [[ "${normalized}" != "${bin}" ]]; then
    printf 'governed jankurai path traverses a symlink: %s -> %s\n' "${bin}" "${normalized}" >&2
    exit 1
  fi
  bin_dir="$(dirname "${bin}")"
  export PATH="${bin_dir}:${PATH}"
  resolved="$(command -v jankurai 2>/dev/null || true)"
  if [[ "${resolved}" != "${bin}" ]]; then
    printf 'governed jankurai shadowed: expected %s, resolved %s\n' "${bin}" "${resolved:-missing}" >&2
    exit 1
  fi
  actual="$("${bin}" --version 2>/dev/null || true)"
  actual_sha="$(sha256sum "${bin}" 2>/dev/null | awk '{print $1}')"
  if [[ "${actual}" != "${JERYU_JANKURAI_VERSION}" ]] ||
     [[ "${actual_sha}" != "${JERYU_JANKURAI_SHA256}" ]]; then
    printf 'governed jankurai identity mismatch at %s: version=%s sha256=%s\n' \
      "${bin}" "${actual:-missing}" "${actual_sha:-missing}" >&2
    exit 1
  fi
  if [[ "${JERYU_JANKURAI_ALLOW_TEST_RECEIPT:-0}" == "1" ]]; then
    expected_test=true
    expected_verification=diagnostic-candidate
    expected_governance=diagnostic-candidate
    expected_protected=false
    expected_protection=not-applicable
  fi
  if [[ -n "${JERYU_JANKURAI_RECEIPT:-}" ]]; then
    receipt_candidates=("${JERYU_JANKURAI_RECEIPT}")
  elif [[ "${bin}" == "/home/ubuntu/.jeryu/bin/jankurai" ]]; then
    receipt_candidates=(/home/ubuntu/.jeryu/receipts/jankurai/sha256/*.json)
  else
    printf 'non-governed jankurai requires an explicit installation receipt: %s\n' "${bin}" >&2
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
      --arg digest "${JERYU_JANKURAI_SHA256}" \
      --arg version "${JERYU_JANKURAI_VERSION}" \
      --arg path "${bin}" \
      --arg verification "${expected_verification}" \
      --arg governance "${expected_governance}" \
      --arg protection "${expected_protection}" \
      --argjson protected_main "${expected_protected}" \
      --argjson test_mode "${expected_test}" \
      '.schema == "jeryu.jankurai-installation/v1" and
       .source.remote == $remote and .source.commit == $commit and .source.tag == $tag and
       .source.tree == $tree and .source.archive_sha256 == $archive and
       .source.cargo_lock_sha256 == $lock and .source.verification == $verification and
       .build.rustc == $rustc and .build.cargo == $cargo and
       .build.target_triple == $triple and .build.mode == $mode and
       .build.cargo_net_offline == true and .build.dedicated_cargo_home == true and
       .build.git_global_config_disabled == true and .build.git_system_config_disabled == true and
       .build.git_http_follow_redirects == false and .build.git_terminal_prompt == false and
       .build.jankurai_update_check == false and
       .build.network_scope == "local-forge-source-plus-offline-cargo" and
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
  if [[ "${found_receipt}" -ne 1 ]]; then
    printf 'governed jankurai receipt mismatch: binary=%s test_mode=%s\n' \
      "${bin}" "${expected_test}" >&2
    exit 1
  fi
  export JANKURAI_NO_UPDATE_CHECK=1 GIT_TERMINAL_PROMPT=0
}

require_jankurai
printf 'governed jankurai ok: %s sha256=%s at %s\n' \
  "${JERYU_JANKURAI_VERSION}" "${JERYU_JANKURAI_SHA256}" "${JERYU_GOVERNED_JANKURAI_BIN}"
