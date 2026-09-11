#!/usr/bin/env bash
# Sourced only by install-jankurai.sh. Do not execute.
[[ "${BASH_SOURCE[0]}" != "${0}" ]] || {
  printf '%s: source from install-jankurai.sh\n' "$(basename -- "${BASH_SOURCE[0]}")" >&2
  exit 1
}

die() {
  printf 'install-jankurai: %s\n' "$*" >&2
  exit 1
}

sha256_file() {
  sha256sum "$1" | awk '{print $1}'
}

require_hex() {
  local name="$1" value="$2" length="$3"
  [[ "${value}" =~ ^[0-9a-f]{${length}}$ ]] || die "invalid ${name}"
}

matching_receipt() {
  local receipt leaf receipt_descriptor
  local expected_test=false expected_verification=release-authoritative
  local expected_governance=governed expected_protected=true
  if [[ "${test_mode}" == "1" ]]; then
    expected_test=true
    expected_verification=diagnostic-candidate
    expected_governance=diagnostic-candidate
    expected_protected=false
  fi
  if [[ "${test_mode}" == "1" && -n "${JERYU_INSTALL_TEST_PREBUILT_BINARY:-}" ]]; then
    expected_verification=test-fixture
  fi
  for receipt in "${receipt_dir_fd_path}"/*.json; do
    [[ -e "${receipt}" || -L "${receipt}" ]] || continue
    leaf="$(basename -- "${receipt}")"
    [[ -f "${receipt}" && ! -L "${receipt}" &&
       "$(stat -Lc '%h' -- "${receipt}")" == "1" &&
       "$(realpath -e -- "${receipt}")" == "${receipt_dir}/${leaf}" ]] || continue
    receipt_descriptor="${receipt}"
    if [[ "${public_candidate}" == 1 ]]; then
      if candidate_receipt_valid "${receipt_descriptor}"; then
        printf '%s' "${receipt_dir}/${leaf}"
        return 0
      fi
      continue
    fi
    if jq -e \
      --arg remote "${JANKURAI_REPO}" \
      --arg commit "${JANKURAI_REV}" \
      --arg tag "${JANKURAI_TAG}" \
      --arg tree "${JANKURAI_SOURCE_TREE}" \
      --arg archive "${JANKURAI_SOURCE_ARCHIVE_SHA256}" \
      --arg lock "${JANKURAI_CARGO_LOCK_SHA256}" \
      --arg rustc "${JANKURAI_RUSTC_VERSION}" \
      --arg cargo "${JANKURAI_CARGO_VERSION}" \
      --arg triple "${JANKURAI_TARGET_TRIPLE}" \
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
      --arg digest "${JANKURAI_BINARY_SHA256}" \
      --arg version "${JANKURAI_VERSION}" \
      --arg path "${target}" \
      --arg install_lock_path "${install_lock_path}" \
      --arg install_lock_identity "${install_lock_identity}" \
      --arg verification "${expected_verification}" \
      --arg manifest_repo "${manifest_repo}" \
      --arg manifest_commit "${manifest_commit}" \
      --arg manifest_tree "${manifest_tree}" \
      --arg manifest_sha "${manifest_sha256}" \
      --arg governance "${expected_governance}" \
      --arg protection "${governance_protection}" \
      --argjson protected_main "${expected_protected}" \
      --argjson test_mode "${expected_test}" \
      '.schema == "jeryu.jankurai-installation/v2" and
       .source.remote == $remote and .source.commit == $commit and .source.tag == $tag and
       .source.tree == $tree and .source.archive_sha256 == $archive and
       .source.cargo_lock_sha256 == $lock and .source.verification == $verification and
       .build.rustc == $rustc and
       .build.cargo == $cargo and .build.target_triple == $triple and
       .build.mode == $mode and .build.package_path == $package_path and
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
       .build.git_global_config_disabled == true and
       .build.git_system_config_disabled == true and
       .build.git_http_follow_redirects == false and
       .build.git_terminal_prompt == false and
       .build.jankurai_update_check == false and
       .build.network_scope ==
         "local-forge-source-plus-closed-vendor-network-none" and
       .build.no_proxy == "127.0.0.1,localhost,::1" and
       .governance.status == $governance and
       .governance.manifest_repo == $manifest_repo and
       .governance.manifest_commit == $manifest_commit and
       .governance.manifest_tree == $manifest_tree and
       .governance.manifest_sha256 == $manifest_sha and
       .governance.protected_main == $protected_main and
       .governance.protection_policy == $protection and
       .binary.sha256 == $digest and
       .binary.version_output == $version and .installation.path == $path and
       .installation.atomic == true and .installation.lock.exclusive == true and
       .installation.lock.path == $install_lock_path and
       .installation.lock.identity == $install_lock_identity and
       .installation.lock.held_through_receipt == true and
       .conclusion == "success" and
       .test_mode == $test_mode' "${receipt_descriptor}" >/dev/null 2>&1; then
      printf '%s' "${receipt_dir}/${leaf}"
      return 0
    fi
  done
  return 1
}

install_jankurai_reuse_or_scratch() {
  require_transaction_custody
  if [[ -e "${target_custody_path}" || -L "${target_custody_path}" ]]; then
    existing_target_fd=""
    open_custody_file "${install_dir_fd}" "${install_dir}" jankurai \
      existing_target_fd _ignored_file_identity ||
      die "existing target is not a single-link physical regular file"
    existing_target_descriptor="/proc/${installer_pid}/fd/${existing_target_fd}"
    existing_sha="$(sha256_file "${existing_target_descriptor}")"
    existing_version=""
    if [[ "${public_candidate}" != 1 || "${existing_sha}" == "${JANKURAI_BINARY_SHA256}" ]]; then
      existing_version="$("${existing_target_descriptor}" --version 2>/dev/null || true)"
    fi
    if [[ "${existing_version}" == "${JANKURAI_VERSION}" &&
          "${existing_sha}" == "${JANKURAI_BINARY_SHA256}" ]]; then
      if receipt="$(matching_receipt)"; then
        receipt_leaf="$(basename "${receipt}")"
        receipt_digest="${receipt_leaf%.json}"
        existing_receipt_fd=""
        open_custody_file "${receipt_dir_fd}" "${receipt_dir}" "${receipt_leaf}" \
          existing_receipt_fd _ignored_file_identity ||
          die "content-addressed receipt lost physical custody: ${receipt}"
        [[ "$(sha256_file "/proc/${installer_pid}/fd/${existing_receipt_fd}")" == \
           "${receipt_digest}" ]] ||
          die "content-addressed receipt failed self-verification: ${receipt}"
        require_transaction_custody
        if [[ "${public_candidate}" == 1 ]]; then
          candidate_recheck
          candidate_public_readback
          candidate_receipt_valid "/proc/${installer_pid}/fd/${existing_receipt_fd}" ||
            die "retained candidate receipt changed"
        fi
        if [[ "${public_candidate}" == 1 ]]; then
          remove_owned_scratch "${candidate_state}" "${candidate_state_identity}" ||
            die "candidate input cleanup failed; existing installation retained"
          candidate_state=""
          require_transaction_custody
          candidate_receipt_custody "${existing_receipt_fd}" "${receipt_digest}" "${receipt}" ||
            die "current candidate receipt changed during final verification"
          [[ "$(realpath -e -- "${existing_target_descriptor}")" == "${target}" &&
             "$(sha256_file "${existing_target_descriptor}")" == "${JANKURAI_BINARY_SHA256}" &&
             "$(stat -Lc '%d:%i:%u:%g:%h' -- "${existing_target_descriptor}")" == \
               "$(stat -Lc '%d:%i:%u:%g:%h' -- "${target_custody_path}")" ]] ||
            die "current candidate binary changed during verification"
          jq -nc --arg receipt "${receipt}" --arg path "${target}" --arg sha256 "${existing_sha}" \
            '{status:"current",receipt:$receipt,path:$path,sha256:$sha256}'
        else
          printf 'jeryu jankurai already current: %s sha256=%s receipt=%s\n' \
            "${JANKURAI_VERSION}" "${existing_sha}" "${receipt}"
        fi
        exit 0
      fi
    fi
  fi

  scratch="$(mktemp -d /tmp/jeryu-install-jankurai.XXXXXX)"
  scratch_identity="$(stat -c '%d:%i:%u' -- "${scratch}")"
  stage_fd=""
  stage_leaf=""
  stage_identity=""
  backup_stage_fd=""
  backup_stage_leaf=""
  backup_stage_identity=""
  receipt_install_fd=""
  receipt_install_leaf=""
  receipt_install_identity=""
  previous_backup=""
  previous_backup_fd=""
  previous_sha=""
  target_replaced=0
  receipt_published=0
  installed_target_identity=""
  success=0
  builder_in_flight=0
}

rollback_target() {
  local restore_fd restore_leaf restore_identity restore_descriptor
  validate_install_lock || return 1
  if [[ -n "${previous_backup}" ]]; then
    [[ -n "${previous_backup_fd}" &&
       "$(sha256_file "/proc/${installer_pid}/fd/${previous_backup_fd}")" == \
         "${previous_sha}" ]] || return 1
    create_exclusive_leaf "${install_dir_fd}" jankurai.rollback \
      restore_fd restore_leaf restore_identity
    restore_descriptor="/proc/${installer_pid}/fd/${restore_fd}"
    cat "/proc/${installer_pid}/fd/${previous_backup_fd}" >&"${restore_fd}"
    chmod 755 "${restore_descriptor}"
    [[ "$(sha256_file "${restore_descriptor}")" == "${previous_sha}" ]] || return 1
    sync -f "${restore_descriptor}"
    validate_retained_leaf "${restore_fd}" "${restore_identity}" \
      "${install_dir_fd}" "${restore_leaf}" || return 1
    mv -fT "${install_dir_fd_path}/${restore_leaf}" "${target_custody_path}"
    [[ "$(stat -Lc '%d:%i:%u:%g:%h' -- "${target_custody_path}")" == \
       "${restore_identity}" ]] || return 1
    [[ "$(sha256_file "${target_custody_path}")" == "${previous_sha}" ]] || return 1
  else
    [[ "$(stat -Lc '%d:%i:%u:%g:%h' -- "${target_custody_path}")" == \
       "${installed_target_identity}" ]] || return 1
    rm -f -- "${target_custody_path}"
  fi
  sync -f "${install_dir_fd_path}"
}

finish() {
  local status=$? keep_candidate_state=0 log_fd log_leaf log_identity
  trap - EXIT
  if [[ "${status}" -ne 0 && "${target_replaced}" -eq 1 && "${success}" -ne 1 ]]; then
    if [[ "${public_candidate}" == 1 && "${receipt_published}" == 1 ]]; then
      if validate_retained_leaf "${receipt_fd}" "${receipt_install_identity}" \
        "${receipt_dir_fd}" "${receipt_leaf}"; then
        if ! rm -f -- "${receipt_dir_fd_path}/${receipt_leaf}" ||
           ! sync -f "${receipt_dir_fd_path}"; then
          printf 'install-jankurai: failed to withdraw transaction receipt; inspect retained evidence\n' >&2
        fi
      else
        printf 'install-jankurai: failed transaction receipt custody changed; inspect retained evidence\n' >&2
      fi
    fi
    validate_install_lock && rollback_target ||
      printf 'install-jankurai: rollback verification failed; retained only verified target bytes\n' >&2
  fi
  if [[ -n "${stage_fd}" && -n "${stage_leaf}" ]]; then
    remove_retained_leaf "${stage_fd}" "${stage_identity}" "${install_dir_fd}" "${stage_leaf}"
  fi
  if [[ -n "${backup_stage_fd}" && -n "${backup_stage_leaf}" ]]; then
    remove_retained_leaf "${backup_stage_fd}" "${backup_stage_identity}" \
      "${rollback_dir_fd}" "${backup_stage_leaf}"
  fi
  if [[ -n "${receipt_install_fd}" && -n "${receipt_install_leaf}" ]]; then
    remove_retained_leaf "${receipt_install_fd}" "${receipt_install_identity}" \
      "${receipt_dir_fd}" "${receipt_install_leaf}"
  fi
  if [[ "${status}" -ne 0 && "${public_candidate}" == 1 && -n "${candidate_state}" &&
        -e "${candidate_state}/build.log" ]]; then
    # Keep diagnostics under the already-held root; never expose cache contents.
    if [[ -f "${candidate_state}/build.log" && ! -L "${candidate_state}/build.log" &&
          "$(stat -c '%a:%u:%h' -- "${candidate_state}/build.log")" == "600:$(id -u):1" ]] &&
       validate_install_lock && candidate_ancestors; then
      create_exclusive_leaf "${install_root_fd}" jankurai-build-failure log_fd log_leaf log_identity
      cat "${candidate_state}/build.log" >&"${log_fd}"
      sync -f "/proc/${installer_pid}/fd/${log_fd}"
      validate_retained_leaf "${log_fd}" "${log_identity}" "${install_root_fd}" "${log_leaf}" ||
        die "failed build log lost physical custody"
      printf 'install-jankurai: retained build log: %s/%s\n' "${install_root}" "${log_leaf}" >&2
      tail -n 20 "/proc/${installer_pid}/fd/${log_fd}" >&2
    else
      printf 'install-jankurai: retained candidate scratch because build log custody changed: %s\n' \
        "${candidate_state}" >&2
      keep_candidate_state=1
    fi
  fi
  if [[ "${builder_in_flight}" == 1 ]]; then
    # Concurrent parent/child TERM traps must never remove a live builder bind.
    printf 'install-jankurai: retaining source and candidate state after incomplete builder call: %s\n' "${scratch}" >&2
    keep_candidate_state=1
    status=1
  elif [[ -n "${scratch}" ]]; then
    remove_owned_scratch "${scratch}" "${scratch_identity}" || {
      printf 'install-jankurai: retained changed or mounted build scratch\n' >&2
      status=1
    }
  fi
  if [[ -n "${candidate_state}" && "${keep_candidate_state}" == 0 ]]; then
    remove_owned_scratch "${candidate_state}" "${candidate_state_identity}" || {
      printf 'install-jankurai: retained changed or mounted candidate inputs\n' >&2
      status=1
    }
  fi
  exit "${status}"
}
