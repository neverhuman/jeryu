#!/usr/bin/env bash
# Sourced only by install-jankurai.sh. Do not execute.
[[ "${BASH_SOURCE[0]}" != "${0}" ]] || {
  printf '%s: source from install-jankurai.sh\n' "$(basename -- "${BASH_SOURCE[0]}")" >&2
  exit 1
}

validate_custody_dir() {
  local fd="$1" identity="$2" public_path="$3"
  local fd_path="/proc/${installer_pid}/fd/${fd}"
  local descriptor_identity path_identity physical_path mode
  [[ -d "${public_path}" && ! -L "${public_path}" ]] || return 1
  descriptor_identity="$(stat -Lc '%d:%i:%u:%g:%a' -- "${fd_path}")" || return 1
  path_identity="$(stat -Lc '%d:%i:%u:%g:%a' -- "${public_path}")" || return 1
  physical_path="$(realpath -e -- "${fd_path}")" || return 1
  mode="$(stat -Lc '%a' -- "${fd_path}")" || return 1
  [[ "${descriptor_identity}" == "${identity}" &&
     "${path_identity}" == "${identity}" &&
     "${physical_path}" == "${public_path}" &&
     "$(stat -Lc '%u:%g' -- "${fd_path}")" == "$(id -u):$(id -g)" ]] || return 1
  (( (8#${mode} & 8#022) == 0 ))
}

open_custody_dir() {
  local parent_fd="$1" public_parent="$2" component="$3"
  local output_fd="$4" output_identity="$5"
  local parent_fd_path="/proc/${installer_pid}/fd/${parent_fd}"
  local child_fd_path public_child child_identity
  [[ "${component}" =~ ^[A-Za-z0-9._-]+$ && "${component}" != "." &&
     "${component}" != ".." ]] || die "invalid install directory component"
  public_child="${public_parent}/${component}"
  if [[ ! -e "${parent_fd_path}/${component}" && ! -L "${parent_fd_path}/${component}" ]]; then
    mkdir -- "${parent_fd_path}/${component}" 2>/dev/null || true
  fi
  [[ -d "${parent_fd_path}/${component}" && ! -L "${parent_fd_path}/${component}" ]] ||
    die "install directory is not a physical directory: ${public_child}"
  exec {_opened_custody_fd}<"${parent_fd_path}/${component}"
  child_fd_path="/proc/${installer_pid}/fd/${_opened_custody_fd}"
  child_identity="$(stat -Lc '%d:%i:%u:%g:%a' -- "${child_fd_path}")"
  validate_custody_dir "${_opened_custody_fd}" "${child_identity}" "${public_child}" ||
    die "install directory escaped physical custody: ${public_child}"
  printf -v "${output_fd}" '%s' "${_opened_custody_fd}"
  printf -v "${output_identity}" '%s' "${child_identity}"
}

validate_install_root() {
  validate_custody_dir "${install_root_fd}" "${install_root_identity}" "${install_root}"
}

install_jankurai_bind_custody() {
  install_lock_custody_path="${install_root_fd_path}/.jankurai-install.lock"
  if [[ ! -e "${install_lock_custody_path}" && ! -L "${install_lock_custody_path}" ]]; then
    (
      set -o noclobber
      : > "${install_lock_custody_path}"
    ) 2>/dev/null || true
  fi
  [[ -f "${install_lock_custody_path}" && ! -L "${install_lock_custody_path}" ]] ||
    die "installation lock is not a physical regular file"
  lock_uid="$(stat -Lc '%u' -- "${install_lock_custody_path}")"
  lock_gid="$(stat -Lc '%g' -- "${install_lock_custody_path}")"
  lock_mode="$(stat -Lc '%a' -- "${install_lock_custody_path}")"
  lock_links="$(stat -Lc '%h' -- "${install_lock_custody_path}")"
  [[ "${lock_uid}" == "$(id -u)" && "${lock_gid}" == "$(id -g)" ]] ||
    die "installation lock is not owned by the current identity"
  [[ "${lock_mode}" == "600" && "${lock_links}" == "1" ]] ||
    die "installation lock must be mode 0600 and single-link"

  command -v flock >/dev/null 2>&1 || die "flock is required for installation custody"
  exec {install_lock_fd}<"${install_lock_custody_path}"
  if [[ "${test_mode}" == "1" && -n "${JERYU_INSTALL_TEST_LOCK_WAITING_FILE:-}" ]]; then
    [[ "${JERYU_INSTALL_TEST_LOCK_WAITING_FILE}" == /* ]] ||
      die "test lock waiting marker must be absolute"
    printf 'waiting\n' > "${JERYU_INSTALL_TEST_LOCK_WAITING_FILE}"
  fi
  if [[ "${public_candidate}" == 1 ]]; then
    flock -x --timeout 30 "${install_lock_fd}" || die "public candidate install lock contended for 30 seconds"
  else
    flock -x "${install_lock_fd}" || die "unable to acquire exclusive installation lock"
  fi
  install_lock_identity="$(stat -Lc '%d:%i:%u:%g:%a:%h' -- \
    "/proc/${installer_pid}/fd/${install_lock_fd}")"

  validate_install_lock() {
    local descriptor_identity custody_identity path_identity
    validate_install_root || return 1
    [[ -f "${install_lock_path}" && ! -L "${install_lock_path}" ]] || return 1
    descriptor_identity="$(stat -Lc '%d:%i:%u:%g:%a:%h' -- \
      "/proc/${installer_pid}/fd/${install_lock_fd}")" || return 1
    custody_identity="$(stat -Lc '%d:%i:%u:%g:%a:%h' -- \
      "${install_lock_custody_path}")" || return 1
    path_identity="$(stat -Lc '%d:%i:%u:%g:%a:%h' -- "${install_lock_path}")" || return 1
    [[ "${descriptor_identity}" == "${install_lock_identity}" &&
       "${custody_identity}" == "${install_lock_identity}" &&
       "${path_identity}" == "${install_lock_identity}" ]]
  }

  require_install_lock() {
    validate_install_lock || die "exclusive installation lock custody changed"
  }

  require_install_lock
  if [[ "${test_mode}" == "1" && -n "${JERYU_INSTALL_TEST_LOCK_ACQUIRED_FILE:-}" ]]; then
    [[ "${JERYU_INSTALL_TEST_LOCK_ACQUIRED_FILE}" == /* ]] ||
      die "test lock acquired marker must be absolute"
    printf 'acquired\n' > "${JERYU_INSTALL_TEST_LOCK_ACQUIRED_FILE}"
  fi

  install_dir_fd=""
  install_dir_identity=""
  receipts_root_fd=""
  receipts_root_identity=""
  receipts_jankurai_fd=""
  receipts_jankurai_identity=""
  receipt_dir_fd=""
  receipt_dir_identity=""
  rollback_root_fd=""
  rollback_root_identity=""
  rollback_dir_fd=""
  rollback_dir_identity=""
  open_custody_dir "${install_root_fd}" "${install_root}" bin \
    install_dir_fd install_dir_identity
  open_custody_dir "${install_root_fd}" "${install_root}" receipts \
    receipts_root_fd receipts_root_identity
  open_custody_dir "${receipts_root_fd}" "${install_root}/receipts" jankurai \
    receipts_jankurai_fd receipts_jankurai_identity
  open_custody_dir "${receipts_jankurai_fd}" "${install_root}/receipts/jankurai" sha256 \
    receipt_dir_fd receipt_dir_identity
  open_custody_dir "${install_root_fd}" "${install_root}" rollback \
    rollback_root_fd rollback_root_identity
  open_custody_dir "${rollback_root_fd}" "${install_root}/rollback" jankurai \
    rollback_dir_fd rollback_dir_identity

  install_dir_fd_path="/proc/${installer_pid}/fd/${install_dir_fd}"
  receipt_dir_fd_path="/proc/${installer_pid}/fd/${receipt_dir_fd}"
  rollback_dir_fd_path="/proc/${installer_pid}/fd/${rollback_dir_fd}"
  target_custody_path="${install_dir_fd_path}/jankurai"

  require_transaction_custody() {
    if [[ "${public_candidate}" == 1 ]]; then
      candidate_ancestors || die "candidate installation ancestor custody changed"
    fi
    require_install_lock
    if ! validate_custody_dir "${install_dir_fd}" "${install_dir_identity}" "${install_dir}" ||
      ! validate_custody_dir "${receipts_root_fd}" "${receipts_root_identity}" \
        "${install_root}/receipts" ||
      ! validate_custody_dir "${receipts_jankurai_fd}" "${receipts_jankurai_identity}" \
        "${install_root}/receipts/jankurai" ||
      ! validate_custody_dir "${receipt_dir_fd}" "${receipt_dir_identity}" "${receipt_dir}" ||
      ! validate_custody_dir "${rollback_root_fd}" "${rollback_root_identity}" \
        "${install_root}/rollback" ||
      ! validate_custody_dir "${rollback_dir_fd}" "${rollback_dir_identity}" "${rollback_dir}"; then
      die "physical install transaction custody changed"
    fi
  }

  test_pause() {
    local ready_file="$1" release_file="$2" purpose="$3"
    local released=0
    [[ "${test_mode}" == "1" ]] || die "${purpose} pause is test-only"
    [[ -n "${ready_file}" && -n "${release_file}" &&
       "${ready_file}" == /* && "${release_file}" == /* ]] ||
      die "${purpose} pause requires absolute ready and release files"
    printf 'ready\n' > "${ready_file}"
    for _ in {1..1000}; do
      if [[ -e "${release_file}" ]]; then
        released=1
        break
      fi
      sleep 0.01
    done
    [[ "${released}" == "1" ]] || die "timed out waiting to release ${purpose} pause"
  }

  require_transaction_custody
  if [[ "${test_mode}" == "1" &&
        ( -n "${JERYU_INSTALL_TEST_PAUSE_AFTER_CUSTODY_READY_FILE:-}" ||
          -n "${JERYU_INSTALL_TEST_PAUSE_AFTER_CUSTODY_RELEASE_FILE:-}" ) ]]; then
    test_pause "${JERYU_INSTALL_TEST_PAUSE_AFTER_CUSTODY_READY_FILE:-}" \
      "${JERYU_INSTALL_TEST_PAUSE_AFTER_CUSTODY_RELEASE_FILE:-}" "transaction-custody"
    require_transaction_custody
  fi

  export GIT_CONFIG_GLOBAL=/dev/null
  export GIT_CONFIG_NOSYSTEM=1
  export GIT_TERMINAL_PROMPT=0
  export JANKURAI_NO_UPDATE_CHECK=1
  export CARGO_NET_OFFLINE=true
  export NO_PROXY="127.0.0.1,localhost,::1"
  export no_proxy="${NO_PROXY}"

  if [[ "${public_candidate}" != 1 ]]; then
    token_file="${JERYU_FORGE_TOKEN_FILE:-/home/ubuntu/.jeryu/secrets/merge-token}"
    [[ -r "${token_file}" ]] || die "local-forge credential is unavailable"
    forge_token="$(tr -d '\n' < "${token_file}")"
    [[ -n "${forge_token}" ]] || die "local-forge credential is empty"
  fi
  git_bin=git
  if [[ -n "${JERYU_INSTALL_TEST_GIT_BIN:-}" ]]; then
    [[ "${test_mode}" == "1" ]] || die "a Git test double is allowed only in explicit test mode"
    [[ -x "${JERYU_INSTALL_TEST_GIT_BIN}" ]] || die "Git test double is not executable"
    git_bin="${JERYU_INSTALL_TEST_GIT_BIN}"
  fi
  forge_git() {
    if [[ "${public_candidate}" == 1 ]]; then
      public_git "$@"
      return
    fi
    GIT_CONFIG_COUNT=2 \
    GIT_CONFIG_KEY_0=http.extraHeader \
    GIT_CONFIG_VALUE_0="Authorization: Bearer ${forge_token}" \
    GIT_CONFIG_KEY_1=http.followRedirects \
    GIT_CONFIG_VALUE_1=false \
      "${git_bin}" "$@"
  }

  # Bind the installation to the exact jeryu-tool manifest checkout that
  # authorized it. Production installation is permitted only from a clean local
  # checkout of the exact protected main commit, with immutable-main read back
  # from the forge. Candidate qualification records the same Git identity but is
  # explicitly diagnostic and cannot be mistaken for governed installation.
  manifest_root="$(realpath -m "${here}/..")"
  manifest_repo="http://127.0.0.1:8787/git/jeryu/jeryu-tool.git"
  if [[ "${public_candidate}" == 1 ]]; then
    manifest_repo="$(jq -r .source.repository "${candidate_state}/renderer.json")"
    manifest_commit="${expected_head}"
    manifest_sha256="${candidate_manifest_sha}"
  else
    manifest_commit="$(git -C "${manifest_root}" rev-parse HEAD)"
    manifest_tree="$(git -C "${manifest_root}" rev-parse 'HEAD^{tree}')"
    manifest_sha256="$(sha256_file "${manifest_root}/tool-manifest.toml")"
  fi
  require_hex JERYU_TOOL_MANIFEST_COMMIT "${manifest_commit}" 40
  require_hex JERYU_TOOL_MANIFEST_TREE "${manifest_tree}" 40
  require_hex JERYU_TOOL_MANIFEST_SHA256 "${manifest_sha256}" 64
  governance_status="diagnostic-candidate"
  governance_protected_main=false
  governance_protection="not-applicable"
  if [[ "${public_candidate}" == 1 ]]; then
    governance_status="public-candidate"
  elif [[ "${test_mode}" != "1" ]]; then
    [[ "$(git -C "${manifest_root}" remote get-url origin)" == "${manifest_repo}" ]] ||
      die "jeryu-tool manifest origin is not canonical"
    [[ -z "$(git -C "${manifest_root}" status --porcelain --untracked-files=all)" ]] ||
      die "jeryu-tool manifest checkout must be clean for governed installation"
    manifest_remote_main="$(forge_git ls-remote --heads "${manifest_repo}" refs/heads/main |
      awk '$2 == "refs/heads/main" {print $1; exit}')"
    [[ "${manifest_remote_main}" == "${manifest_commit}" ]] ||
      die "jeryu-tool manifest checkout is not exact protected main"
    protection_readback="$(curl -fsS --max-time 15 --max-redirs 0 --proto '=http' \
      -H 'accept: application/json' -H "authorization: Bearer ${forge_token}" \
      'http://127.0.0.1:8787/repos/jeryu/jeryu-tool/branches/main/protection')" ||
      die "unable to read back jeryu-tool branch protection"
    jq -e --arg check "jeryu-tool/required" '
      ((if (.required_status_checks | type) == "array" then .required_status_checks
        else (.required_status_checks.contexts // []) end | index($check)) != null)
      and ((.required_approving_review_count //
        .required_pull_request_reviews.required_approving_review_count // 0) >= 1)
      and ((if (.required_linear_history | type) == "object" then
        .required_linear_history.enabled else .required_linear_history end) == true)
      and ((if (.enforce_admins | type) == "object" then
        .enforce_admins.enabled else .enforce_admins end) == true)
      and ((if (.allow_force_pushes | type) == "object" then
        .allow_force_pushes.enabled else .allow_force_pushes end) == false)
      and ((if (.allow_deletions | type) == "object" then
        .allow_deletions.enabled else .allow_deletions end) == false)
    ' <<<"${protection_readback}" >/dev/null ||
      die "jeryu-tool protection does not satisfy immutable-main-v1"
    governance_status="governed"
    governance_protected_main=true
    governance_protection="immutable-main-v1"
  fi
}

open_custody_file() {
  local parent_fd="$1" public_parent="$2" leaf="$3"
  local output_fd="$4" output_identity="$5"
  local parent_fd_path="/proc/${installer_pid}/fd/${parent_fd}"
  local custody_path="${parent_fd_path}/${leaf}" public_path="${public_parent}/${leaf}"
  local descriptor_path descriptor_identity path_identity
  [[ "${leaf}" =~ ^[A-Za-z0-9._-]+$ && "${leaf}" != "." && "${leaf}" != ".." ]] ||
    return 1
  [[ -f "${custody_path}" && ! -L "${custody_path}" ]] || return 1
  exec {_opened_custody_file_fd}<"${custody_path}" || return 1
  descriptor_path="/proc/${installer_pid}/fd/${_opened_custody_file_fd}"
  descriptor_identity="$(stat -Lc '%d:%i:%u:%g:%h' -- "${descriptor_path}")" || return 1
  path_identity="$(stat -Lc '%d:%i:%u:%g:%h' -- "${custody_path}")" || return 1
  [[ "${descriptor_identity}" == "${path_identity}" &&
     "$(realpath -e -- "${descriptor_path}")" == "${public_path}" &&
     -f "${descriptor_path}" && "$(stat -Lc '%h' -- "${descriptor_path}")" == "1" ]] ||
    return 1
  printf -v "${output_fd}" '%s' "${_opened_custody_file_fd}"
  printf -v "${output_identity}" '%s' "${descriptor_identity}"
}

create_exclusive_leaf() {
  local parent_fd="$1" prefix="$2" output_fd="$3" output_leaf="$4" output_identity="$5"
  local parent_fd_path="/proc/${installer_pid}/fd/${parent_fd}"
  local token leaf custody_path descriptor_path descriptor_identity
  [[ "${prefix}" =~ ^[A-Za-z0-9._-]+$ ]] || die "invalid transaction leaf prefix"
  for _ in {1..32}; do
    IFS= read -r token < /proc/sys/kernel/random/uuid ||
      die "unable to obtain an unpredictable transaction identity"
    [[ "${token}" =~ ^[0-9a-f-]{36}$ ]] ||
      die "kernel returned an invalid transaction identity"
    leaf=".${prefix}.${token}"
    custody_path="${parent_fd_path}/${leaf}"
    set -o noclobber
    if exec {_created_custody_fd}>"${custody_path}"; then
      set +o noclobber
      descriptor_path="/proc/${installer_pid}/fd/${_created_custody_fd}"
      descriptor_identity="$(stat -Lc '%d:%i:%u:%g:%h' -- "${descriptor_path}")"
      [[ -f "${descriptor_path}" && "$(stat -Lc '%h' -- "${descriptor_path}")" == "1" ]] ||
        die "exclusive transaction leaf is not a single-link regular file"
      printf -v "${output_fd}" '%s' "${_created_custody_fd}"
      printf -v "${output_leaf}" '%s' "${leaf}"
      printf -v "${output_identity}" '%s' "${descriptor_identity}"
      return 0
    fi
    set +o noclobber
  done
  die "unable to create an unpredictable exclusive transaction leaf"
}

validate_retained_leaf() {
  local fd="$1" identity="$2" parent_fd="$3" leaf="$4"
  local descriptor_path="/proc/${installer_pid}/fd/${fd}"
  local custody_path="/proc/${installer_pid}/fd/${parent_fd}/${leaf}"
  local descriptor_identity path_identity
  [[ -f "${custody_path}" && ! -L "${custody_path}" ]] || return 1
  descriptor_identity="$(stat -Lc '%d:%i:%u:%g:%h' -- "${descriptor_path}")" || return 1
  path_identity="$(stat -Lc '%d:%i:%u:%g:%h' -- "${custody_path}")" || return 1
  [[ "${descriptor_identity}" == "${identity}" &&
     "${path_identity}" == "${identity}" &&
     -f "${descriptor_path}" && "$(stat -Lc '%h' -- "${descriptor_path}")" == "1" ]]
}

remove_retained_leaf() {
  local fd="$1" identity="$2" parent_fd="$3" leaf="$4"
  if validate_retained_leaf "${fd}" "${identity}" "${parent_fd}" "${leaf}"; then
    rm -f -- "/proc/${installer_pid}/fd/${parent_fd}/${leaf}"
  fi
}

