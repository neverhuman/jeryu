#!/usr/bin/env bash
# Install the governed Jankurai auditor from an immutable local-forge identity.
# Helper functions live in install-jankurai-lib.sh and are sourced from this
# entrypoint so BASH_SOURCE remains the physical installer path.
set -euo pipefail
umask 077

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
for helper in install-jankurai-lib.sh install-jankurai-candidate.sh install-jankurai-custody.sh; do
  lib="${here}/${helper}"
  [[ -f "${lib}" && ! -L "${lib}" && "$(realpath -e -- "${lib}")" == "${lib}" ]] || {
    printf 'install-jankurai: missing physical installer helper %s\n' "${helper}" >&2
    exit 1
  }
  # shellcheck source=/dev/null
  source "${lib}"
done

public_candidate=0
expected_head=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --public-candidate)
      [[ "${public_candidate}" == 0 ]] || die "duplicate public candidate mode"
      public_candidate=1
      shift
      ;;
    --expected-head)
      [[ $# -ge 2 && -z "${expected_head}" ]] || die "expected head requires one value"
      expected_head="$2"
      shift 2
      ;;
    *) die "usage: $0 [--public-candidate --expected-head FULL_SHA]" ;;
  esac
done
if [[ "${public_candidate}" == 1 ]]; then
  require_hex expected-head "${expected_head}" 40
  [[ "$(id -u)" != 0 ]] || die "public candidate installation must run as a non-root user"
  while IFS= read -r name; do
    case "${name}" in
      JERYU_INSTALL_TEST_*|JERYU_PIN_ENV|JERYU_FORGE_TOKEN_FILE|JANKURAI_*|GIT_*|SSH_ASKPASS*)
        die "public candidate mode rejects authority, credential, and test overrides" ;;
    esac
  done < <(compgen -e)
else
  [[ -z "${expected_head}" ]] || die "expected head requires public candidate mode"
fi

candidate_state=""
candidate_state_identity=""
candidate_pin_blob="" candidate_pin_sha=""
candidate_builder_blob="" candidate_builder_sha=""
candidate_predicate_blob="" candidate_predicate_sha=""
candidate_manifest_blob="" candidate_manifest_sha=""

# Helper files define functions only. Sequential install continues here after
# args are known so BASH_SOURCE stays this physical installer path.
install_jankurai_load_pin_and_root
install_jankurai_bind_custody

trap finish EXIT
trap 'exit 130' INT TERM HUP

install_jankurai_reuse_or_scratch

actual_rustc="${JANKURAI_RUSTC_VERSION}"
actual_cargo="${JANKURAI_CARGO_VERSION}"
actual_target="${JANKURAI_TARGET_TRIPLE}"

candidate="${scratch}/out/bin/jankurai"
source_verification="release-authoritative"
if [[ "${public_candidate}" == 1 ]]; then
  source_verification="public-candidate"
elif [[ "${test_mode}" == "1" ]]; then
  source_verification="diagnostic-candidate"
fi
if [[ "${test_mode}" == "1" && -n "${JERYU_INSTALL_TEST_PREBUILT_BINARY:-}" ]]; then
  [[ -x "${JERYU_INSTALL_TEST_PREBUILT_BINARY}" ]] || die "test binary is not executable"
  mkdir -p "$(dirname "${candidate}")"
  cp "${JERYU_INSTALL_TEST_PREBUILT_BINARY}" "${candidate}"
  source_verification="test-fixture"
else
  remote_tag="$(forge_git ls-remote --tags "${JANKURAI_REPO}" \
    "refs/tags/${JANKURAI_TAG}" "refs/tags/${JANKURAI_TAG}^{}" |
    awk -v direct="refs/tags/${JANKURAI_TAG}" -v peeled="refs/tags/${JANKURAI_TAG}^{}" '
      $2 == peeled { print $1; found = 1; exit }
      $2 == direct { direct_rev = $1 }
      END { if (!found && direct_rev != "") print direct_rev }
    ' | head -n 1)"
  [[ "${remote_tag}" == "${JANKURAI_REV}" ]] ||
    die "remote tag mismatch: got ${remote_tag:-missing}, want ${JANKURAI_REV}"

  forge_git -C "${scratch}" init -q source
  forge_git -C "${scratch}/source" remote add origin "${JANKURAI_REPO}"
  [[ "$(forge_git -C "${scratch}/source" remote get-url origin)" == "${JANKURAI_REPO}" ]] ||
    die "source remote changed during checkout"
  forge_git -C "${scratch}/source" fetch -q --no-tags --depth 1 origin \
    "refs/tags/${JANKURAI_TAG}:refs/tags/${JANKURAI_TAG}"
  forge_git -C "${scratch}/source" checkout -q --detach "${JANKURAI_REV}"
  [[ "$(forge_git -C "${scratch}/source" rev-parse HEAD)" == "${JANKURAI_REV}" ]] ||
    die "checked-out commit mismatch"
  [[ "$(forge_git -C "${scratch}/source" rev-parse "refs/tags/${JANKURAI_TAG}^{}")" == "${JANKURAI_REV}" ]] ||
    die "checked-out tag mismatch"
  [[ "$(forge_git -C "${scratch}/source" rev-parse "HEAD^{tree}")" == "${JANKURAI_SOURCE_TREE}" ]] ||
    die "source tree mismatch"
  archive_sha="$(forge_git -C "${scratch}/source" archive --format=tar HEAD | sha256sum | awk '{print $1}')"
  [[ "${archive_sha}" == "${JANKURAI_SOURCE_ARCHIVE_SHA256}" ]] ||
    die "source archive mismatch"
  [[ "$(sha256_file "${scratch}/source/Cargo.lock")" == "${JANKURAI_CARGO_LOCK_SHA256}" ]] ||
    die "Cargo.lock mismatch"
  [[ -z "$(forge_git -C "${scratch}/source" status --porcelain --untracked-files=all)" ]] ||
    die "source checkout is dirty before build"

  mkdir -p "$(dirname "${candidate}")"
  builder_in_flight=1
  if [[ "${public_candidate}" == 1 ]]; then
    candidate_recheck
    candidate_prepare_build_cache >"${candidate_state}/build.log" 2>&1
    (
      cd "${candidate_state}"
      env -i PATH="${candidate_cargo_dir}:/usr/bin:/bin" HOME="${candidate_state}/home" \
        CARGO_HOME="${candidate_state}/cargo-home" RUSTUP_HOME="${candidate_rustup_home}" \
        GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 \
        GIT_CONFIG_COUNT=0 GIT_TERMINAL_PROMPT=0 GIT_NO_REPLACE_OBJECTS=1 \
        CARGO_NET_OFFLINE=true JANKURAI_NO_UPDATE_CHECK=1 \
        JERYU_PIN_ENV="${candidate_state}/pin.env" /usr/bin/bash "${candidate_state}/builder.sh" \
        "${scratch}/source" "${candidate}"
    ) >>"${candidate_state}/build.log" 2>&1
    tail -n 20 "${candidate_state}/build.log" >&2
  else
    "${here}/build-jankurai-hermetic.sh" "${scratch}/source" "${candidate}"
  fi
  builder_in_flight=0
  [[ -z "$(forge_git -C "${scratch}/source" status --porcelain --untracked-files=all)" ]] ||
    die "source checkout became dirty during build"
fi

candidate_version="$("${candidate}" --version 2>/dev/null || true)"
candidate_sha="$(sha256_file "${candidate}")"
[[ "${candidate_version}" == "${JANKURAI_VERSION}" ]] ||
  die "built version mismatch: got ${candidate_version:-missing}, want ${JANKURAI_VERSION}"
[[ "${candidate_sha}" == "${JANKURAI_BINARY_SHA256}" ]] ||
  die "built digest mismatch: got ${candidate_sha}, want ${JANKURAI_BINARY_SHA256}"

require_transaction_custody
if [[ -e "${target_custody_path}" || -L "${target_custody_path}" ]]; then
  previous_target_fd=""
  open_custody_file "${install_dir_fd}" "${install_dir}" jankurai \
    previous_target_fd _ignored_file_identity ||
    die "existing target is not a single-link physical regular file"
  previous_target_descriptor="/proc/${installer_pid}/fd/${previous_target_fd}"
  previous_sha="$(sha256_file "${previous_target_descriptor}")"
  previous_backup="${rollback_dir}/${previous_sha}"
  previous_backup_leaf="${previous_sha}"
  if [[ -e "${rollback_dir_fd_path}/${previous_backup_leaf}" ||
        -L "${rollback_dir_fd_path}/${previous_backup_leaf}" ]]; then
    open_custody_file "${rollback_dir_fd}" "${rollback_dir}" "${previous_backup_leaf}" \
      previous_backup_fd previous_backup_identity ||
      die "rollback artifact is not a single-link physical regular file"
  else
    create_exclusive_leaf "${rollback_dir_fd}" "${previous_sha}.stage" \
      backup_stage_fd backup_stage_leaf backup_stage_identity
    backup_stage_descriptor="/proc/${installer_pid}/fd/${backup_stage_fd}"
    cat "${previous_target_descriptor}" >&"${backup_stage_fd}"
    chmod 755 "${backup_stage_descriptor}"
    [[ "$(sha256_file "${backup_stage_descriptor}")" == "${previous_sha}" ]] ||
      die "rollback copy mismatch"
    sync -f "${backup_stage_descriptor}"
    require_transaction_custody
    validate_retained_leaf "${backup_stage_fd}" "${backup_stage_identity}" \
      "${rollback_dir_fd}" "${backup_stage_leaf}" ||
      die "rollback transaction leaf custody changed"
    mv -fT "${rollback_dir_fd_path}/${backup_stage_leaf}" \
      "${rollback_dir_fd_path}/${previous_backup_leaf}"
    backup_stage_leaf=""
    [[ "$(stat -Lc '%d:%i:%u:%g:%h' -- \
      "${rollback_dir_fd_path}/${previous_backup_leaf}")" == "${backup_stage_identity}" ]] ||
      die "rollback publication identity changed"
    previous_backup_fd="${backup_stage_fd}"
    sync -f "${rollback_dir_fd_path}"
  fi
  [[ "$(sha256_file "/proc/${installer_pid}/fd/${previous_backup_fd}")" == \
     "${previous_sha}" ]] ||
    die "rollback artifact digest mismatch"
fi

require_transaction_custody
create_exclusive_leaf "${install_dir_fd}" jankurai.stage \
  stage_fd stage_leaf stage_identity
stage_descriptor="/proc/${installer_pid}/fd/${stage_fd}"
cat "${candidate}" >&"${stage_fd}"
chmod 755 "${stage_descriptor}"
[[ "$(sha256_file "${stage_descriptor}")" == "${JANKURAI_BINARY_SHA256}" ]] ||
  die "staged digest mismatch"
sync -f "${stage_descriptor}"
if [[ "${test_mode}" == "1" && "${JERYU_INSTALL_TEST_INTERRUPT_BEFORE_RENAME:-0}" == "1" ]]; then
  die "simulated interruption before atomic rename"
fi
if [[ "${test_mode}" == "1" &&
      ( -n "${JERYU_INSTALL_TEST_PAUSE_BEFORE_STAGE_RENAME_READY_FILE:-}" ||
        -n "${JERYU_INSTALL_TEST_PAUSE_BEFORE_STAGE_RENAME_RELEASE_FILE:-}" ) ]]; then
  test_pause "${JERYU_INSTALL_TEST_PAUSE_BEFORE_STAGE_RENAME_READY_FILE:-}" \
    "${JERYU_INSTALL_TEST_PAUSE_BEFORE_STAGE_RENAME_RELEASE_FILE:-}" \
    "pre-stage-rename"
fi
require_transaction_custody
if [[ "${public_candidate}" == 1 ]]; then
  candidate_recheck
  candidate_public_readback
fi
validate_retained_leaf "${stage_fd}" "${stage_identity}" "${install_dir_fd}" "${stage_leaf}" ||
  die "target transaction leaf custody changed"
mv -fT "${install_dir_fd_path}/${stage_leaf}" "${target_custody_path}"
stage_leaf=""
target_replaced=1
installed_target_identity="${stage_identity}"
[[ "$(stat -Lc '%d:%i:%u:%g:%h' -- "${target_custody_path}")" == \
   "${installed_target_identity}" ]] || die "installed target identity changed"
installed_target_fd=""
installed_target_open_identity=""
open_custody_file "${install_dir_fd}" "${install_dir}" jankurai \
  installed_target_fd installed_target_open_identity ||
  die "installed target could not be retained for verification"
[[ "${installed_target_open_identity}" == "${installed_target_identity}" ]] ||
  die "installed target descriptor identity changed"
exec {stage_fd}>&-
stage_fd=""
installed_target_descriptor="/proc/${installer_pid}/fd/${installed_target_fd}"
sync -f "${install_dir_fd_path}"
require_transaction_custody
if [[ "${test_mode}" == "1" &&
      ( -n "${JERYU_INSTALL_TEST_PAUSE_AFTER_RENAME_READY_FILE:-}" ||
        -n "${JERYU_INSTALL_TEST_PAUSE_AFTER_RENAME_RELEASE_FILE:-}" ) ]]; then
  test_pause "${JERYU_INSTALL_TEST_PAUSE_AFTER_RENAME_READY_FILE:-}" \
    "${JERYU_INSTALL_TEST_PAUSE_AFTER_RENAME_RELEASE_FILE:-}" "post-rename"
  require_transaction_custody
fi
if [[ "${test_mode}" == "1" && "${JERYU_INSTALL_TEST_FAIL_AFTER_RENAME:-0}" == "1" ]]; then
  die "simulated post-rename failure"
fi

installed_version="$("${installed_target_descriptor}" --version 2>/dev/null || true)"
installed_sha="$(sha256_file "${installed_target_descriptor}")"
[[ "${installed_version}" == "${JANKURAI_VERSION}" ]] || die "installed version verification failed"
[[ "${installed_sha}" == "${JANKURAI_BINARY_SHA256}" ]] || die "installed digest verification failed"
[[ "$(realpath -e -- "${installed_target_descriptor}")" == "${target}" ]] ||
  die "installed path verification failed"
[[ "$(stat -Lc '%d:%i:%u:%g:%h' -- "${target_custody_path}")" == \
   "${installed_target_identity}" ]] || die "installed target custody changed"
require_transaction_custody

timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
run_id="${JERYU_RUN_ID:-install-${timestamp}-$$}"
operator="${JERYU_OPERATOR:-${USER:-unknown}}"
receipt_stage="${scratch}/installation-receipt.json"
receipt_schema="jeryu.jankurai-installation/v2"
receipt_network_scope="local-forge-source-plus-closed-vendor-network-none"
receipt_no_proxy="127.0.0.1,localhost,::1"
if [[ "${public_candidate}" == 1 ]]; then
  receipt_schema="jeryu.jankurai-public-candidate-installation/v1"
  receipt_network_scope="public-source-fetch-plus-closed-vendor-network-none"
  receipt_no_proxy=""
fi
jq -n -S \
  --arg schema "${receipt_schema}" \
  --arg network_scope "${receipt_network_scope}" \
  --arg no_proxy "${receipt_no_proxy}" \
  --arg timestamp "${timestamp}" \
  --arg operator "${operator}" \
  --arg run_id "${run_id}" \
  --arg remote "${JANKURAI_REPO}" \
  --arg commit "${JANKURAI_REV}" \
  --arg tag "${JANKURAI_TAG}" \
  --arg tree "${JANKURAI_SOURCE_TREE}" \
  --arg archive "${JANKURAI_SOURCE_ARCHIVE_SHA256}" \
  --arg lock "${JANKURAI_CARGO_LOCK_SHA256}" \
  --arg verification "${source_verification}" \
  --arg rustc "${actual_rustc}" \
  --arg cargo "${actual_cargo}" \
  --arg target_triple "${actual_target}" \
  --arg mode "${JANKURAI_BUILD_MODE}" \
  --arg package_path "${JANKURAI_PACKAGE_PATH}" \
  --arg builder_image "${JANKURAI_BUILDER_IMAGE}" \
  --arg builder_image_id "${JANKURAI_BUILDER_IMAGE_ID}" \
  --arg linker "${JANKURAI_LINKER_VERSION}" \
  --arg glibc "${JANKURAI_GLIBC_VERSION}" \
  --arg vendor "${JANKURAI_VENDOR_FILES_SHA256}" \
  --arg vendor_count "${JANKURAI_VENDOR_FILE_COUNT}" \
  --arg cargo_config "${JANKURAI_CARGO_CONFIG_SHA256}" \
  --arg environment "${JANKURAI_BUILD_ENVIRONMENT}" \
  --arg rustflags "${JANKURAI_RUSTFLAGS}" \
  --arg command "${JANKURAI_BUILD_COMMAND}" \
  --arg context "${JANKURAI_BUILD_CONTEXT_SHA256}" \
  --arg binary_sha "${installed_sha}" \
  --arg version "${installed_version}" \
  --arg path "${target}" \
  --arg install_lock_path "${install_lock_path}" \
  --arg install_lock_identity "${install_lock_identity}" \
  --arg previous_sha "${previous_sha}" \
  --arg rollback_path "${previous_backup}" \
  --arg manifest_repo "${manifest_repo}" \
  --arg manifest_commit "${manifest_commit}" \
  --arg manifest_tree "${manifest_tree}" \
  --arg manifest_sha "${manifest_sha256}" \
  --arg governance_status "${governance_status}" \
  --arg protection "${governance_protection}" \
  --argjson protected_main "${governance_protected_main}" \
  --argjson test_mode "$([[ "${test_mode}" == "1" ]] && printf true || printf false)" \
  '{schema:$schema,timestamp:$timestamp,operator:$operator,run_id:$run_id,test_mode:$test_mode,
    source:{remote:$remote,commit:$commit,tag:$tag,tree:$tree,archive_sha256:$archive,
      cargo_lock_sha256:$lock,verification:$verification},
    build:{rustc:$rustc,cargo:$cargo,target_triple:$target_triple,mode:$mode,
      package_path:$package_path,builder_image:$builder_image,
      builder_image_id:$builder_image_id,linker:$linker,glibc:$glibc,
      vendor_files_sha256:$vendor,vendor_file_count:$vendor_count,
      cargo_config_sha256:$cargo_config,environment:$environment,rustflags:$rustflags,
      command:$command,context_sha256:$context,cargo_net_offline:true,
      closed_vendor:true,network_none:true,read_only_root:true,non_root:true,
      capabilities_dropped:true,no_new_privileges:true,
      container_engine_path:"/usr/bin/docker",git_global_config_disabled:true,
      git_system_config_disabled:true,git_http_follow_redirects:false,
      git_terminal_prompt:false,jankurai_update_check:false,
      network_scope:$network_scope,no_proxy:$no_proxy},
    governance:{status:$governance_status,manifest_repo:$manifest_repo,
      manifest_commit:$manifest_commit,manifest_tree:$manifest_tree,
      manifest_sha256:$manifest_sha,protected_main:$protected_main,
      protection_policy:$protection},
    binary:{sha256:$binary_sha,version_output:$version},
    installation:{path:$path,atomic:true,previous_binary_sha256:$previous_sha,
      rollback_artifact:$rollback_path,
      lock:{path:$install_lock_path,identity:$install_lock_identity,
        exclusive:true,held_through_receipt:true}},conclusion:"success"}' > "${receipt_stage}"
if [[ "${public_candidate}" == 1 ]]; then
  candidate_recheck
  candidate_public_readback
  jq -S --argjson renderer "$(cat "${candidate_state}/renderer.json")" \
    --arg renderer_sha "${candidate_renderer_sha}" \
    --arg producer "$(jq -r .producer_repository "${candidate_state}/renderer.json")" \
    --arg pin_blob "${candidate_pin_blob}" --arg pin_sha "${candidate_pin_sha}" \
    --arg builder_blob "${candidate_builder_blob}" --arg builder_sha "${candidate_builder_sha}" '
    .source.producer_repository = $producer |
    .governance.handover = "pending" |
    .governance.predecessor_authentication = "not-performed" |
    .renderer_metadata = $renderer | .renderer_metadata_sha256 = $renderer_sha |
    .inputs = {
      pin:{path:"components/jeryu-tool/generated/jankurai-pin.env",blob:$pin_blob,sha256:$pin_sha},
      builder:{path:"components/jeryu-tool/ops/build-jankurai-hermetic.sh",blob:$builder_blob,sha256:$builder_sha}} |
    .verification = {build:"verified",installation:"verified",public_readback:"verified"}
  ' "${receipt_stage}" >"${candidate_state}/receipt.json"
  mv -fT "${candidate_state}/receipt.json" "${receipt_stage}"
  candidate_receipt_valid "${receipt_stage}" || die "candidate receipt failed the closed predicate"
fi
receipt_sha="$(sha256_file "${receipt_stage}")"
receipt_path="${receipt_dir}/${receipt_sha}.json"
receipt_leaf="${receipt_sha}.json"
require_transaction_custody
if [[ -e "${receipt_dir_fd_path}/${receipt_leaf}" ||
      -L "${receipt_dir_fd_path}/${receipt_leaf}" ]]; then
  open_custody_file "${receipt_dir_fd}" "${receipt_dir}" "${receipt_leaf}" \
    receipt_fd receipt_identity ||
    die "receipt artifact is not a single-link physical regular file"
else
  create_exclusive_leaf "${receipt_dir_fd}" "${receipt_sha}.stage" \
    receipt_install_fd receipt_install_leaf receipt_install_identity
  receipt_install_descriptor="/proc/${installer_pid}/fd/${receipt_install_fd}"
  cat "${receipt_stage}" >&"${receipt_install_fd}"
  sync -f "${receipt_install_descriptor}"
  require_transaction_custody
  validate_retained_leaf "${receipt_install_fd}" "${receipt_install_identity}" \
    "${receipt_dir_fd}" "${receipt_install_leaf}" ||
    die "receipt transaction leaf custody changed"
  mv -fT "${receipt_dir_fd_path}/${receipt_install_leaf}" \
    "${receipt_dir_fd_path}/${receipt_leaf}"
  receipt_fd="${receipt_install_fd}"
  receipt_published=1
  receipt_install_leaf=""
  [[ "$(stat -Lc '%d:%i:%u:%g:%h' -- "${receipt_dir_fd_path}/${receipt_leaf}")" == \
     "${receipt_install_identity}" ]] || die "receipt publication identity changed"
  sync -f "${receipt_dir_fd_path}"
fi
[[ "$(sha256_file "/proc/${installer_pid}/fd/${receipt_fd}")" == "${receipt_sha}" ]] ||
  die "receipt content address mismatch"
require_transaction_custody
if [[ "${public_candidate}" == 1 ]]; then
  candidate_recheck
  candidate_public_readback
  candidate_receipt_valid "/proc/${installer_pid}/fd/${receipt_fd}" ||
    die "published candidate receipt failed verification"
  # Cleanup is part of this transaction. Failure still triggers rollback and
  # withdrawal of only the receipt published by this attempt.
  remove_owned_scratch "${scratch}" "${scratch_identity}" || die "candidate build scratch cleanup failed"
  scratch=""
  remove_owned_scratch "${candidate_state}" "${candidate_state_identity}" || die "candidate input cleanup failed"
  candidate_state=""
  require_transaction_custody
  candidate_receipt_custody "${receipt_fd}" "${receipt_sha}" "${receipt_path}" ||
    die "published candidate receipt changed during final verification"
  [[ "$(realpath -e -- "${installed_target_descriptor}")" == "${target}" &&
     "$(stat -Lc '%d:%i:%u:%g:%h' -- "${target_custody_path}")" == "${installed_target_identity}" &&
     "$(sha256_file "${installed_target_descriptor}")" == "${JANKURAI_BINARY_SHA256}" ]] ||
    die "published candidate binary changed during final verification"
fi

if [[ "${public_candidate}" == 1 ]]; then
  jq -nc --arg receipt "${receipt_path}" --arg path "${target}" --arg sha256 "${installed_sha}" \
    '{status:"installed",receipt:$receipt,path:$path,sha256:$sha256}'
  success=1
else
  success=1
  printf 'jeryu jankurai installed: %s sha256=%s path=%s receipt=%s\n' \
    "${installed_version}" "${installed_sha}" "${target}" "${receipt_path}"
fi
