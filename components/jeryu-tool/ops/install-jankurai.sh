#!/usr/bin/env bash
# Install the governed Jankurai auditor from an immutable local-forge identity.
set -euo pipefail
umask 077

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

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
default_pin_env="${here}/../generated/jankurai-pin.env"
test_mode="${JERYU_INSTALL_TEST_MODE:-0}"
[[ "${test_mode}" == "0" || "${test_mode}" == "1" ]] || die "test mode must be 0 or 1"

governed_install_root="/home/ubuntu/.jeryu"
install_root_input="${JERYU_INSTALL_ROOT:-${governed_install_root}}"
[[ "${install_root_input}" == /* ]] || die "installation root must be absolute"
if [[ "${install_root_input}" != "/" ]]; then
  install_root_input="${install_root_input%/}"
fi
install_root="$(realpath -m -- "${install_root_input}")"
[[ "${install_root}" == "${install_root_input}" ]] ||
  die "installation root must be canonical and symlink-free"
if [[ "${test_mode}" == "1" && "${install_root}" == "${governed_install_root}" ]]; then
  die "test mode must not target the governed installation root"
fi
if [[ "${test_mode}" != "1" && "${install_root}" != "${governed_install_root}" ]]; then
  die "governed installation root must be ${governed_install_root}"
fi

pin_env="${JERYU_PIN_ENV:-${default_pin_env}}"
if [[ "${pin_env}" != "${default_pin_env}" && "${test_mode}" != "1" ]]; then
  die "a non-canonical pin file is allowed only in explicit test mode"
fi
[[ -r "${pin_env}" ]] || die "generated pin is missing: ${pin_env}"
# shellcheck source=/dev/null
source "${pin_env}"

required_pin_vars=(
  JANKURAI_REPO JANKURAI_TAG JANKURAI_REV JANKURAI_VERSION JANKURAI_SEMVER
  JANKURAI_SOURCE_TREE JANKURAI_SOURCE_ARCHIVE_SHA256 JANKURAI_CARGO_LOCK_SHA256
  JANKURAI_BINARY_SHA256 JANKURAI_RUST_TOOLCHAIN JANKURAI_RUSTC_VERSION
  JANKURAI_CARGO_VERSION JANKURAI_TARGET_TRIPLE JANKURAI_BUILD_MODE
  JANKURAI_PACKAGE_PATH JANKURAI_BUILDER_IMAGE JANKURAI_BUILDER_IMAGE_ID
  JANKURAI_LINKER_VERSION JANKURAI_GLIBC_VERSION
  JANKURAI_VENDOR_FILES_SHA256 JANKURAI_VENDOR_FILE_COUNT
  JANKURAI_CARGO_CONFIG_SHA256 JANKURAI_BUILD_ENVIRONMENT
  JANKURAI_RUSTFLAGS JANKURAI_BUILD_COMMAND JANKURAI_BUILD_CONTEXT_SHA256
)
for name in "${required_pin_vars[@]}"; do
  [[ -n "${!name:-}" ]] || die "generated pin is missing ${name}"
done
[[ "${JANKURAI_REPO}" == "http://127.0.0.1:8787/git/jeryu/jankurai.git" ]] ||
  die "unapproved Jankurai source: ${JANKURAI_REPO}"
[[ "${JANKURAI_TAG}" != "v1.6.11-deadlang-precision" ]] ||
  die "burned historical tag is not a release source"
require_hex JANKURAI_REV "${JANKURAI_REV}" 40
require_hex JANKURAI_SOURCE_TREE "${JANKURAI_SOURCE_TREE}" 40
require_hex JANKURAI_SOURCE_ARCHIVE_SHA256 "${JANKURAI_SOURCE_ARCHIVE_SHA256}" 64
require_hex JANKURAI_CARGO_LOCK_SHA256 "${JANKURAI_CARGO_LOCK_SHA256}" 64
require_hex JANKURAI_BINARY_SHA256 "${JANKURAI_BINARY_SHA256}" 64
require_hex JANKURAI_VENDOR_FILES_SHA256 "${JANKURAI_VENDOR_FILES_SHA256}" 64
require_hex JANKURAI_CARGO_CONFIG_SHA256 "${JANKURAI_CARGO_CONFIG_SHA256}" 64
require_hex JANKURAI_BUILD_CONTEXT_SHA256 "${JANKURAI_BUILD_CONTEXT_SHA256}" 64
[[ "${JANKURAI_BUILD_MODE}" == "oci-vendor-locked-offline-workspace-member-v2" ]] ||
  die "unsupported build mode: ${JANKURAI_BUILD_MODE}"
[[ "${JANKURAI_PACKAGE_PATH}" == "crates/jankurai" ]] ||
  die "unsupported package path: ${JANKURAI_PACKAGE_PATH}"

install_dir="${install_root}/bin"
target="${install_dir}/jankurai"
receipt_dir="${install_root}/receipts/jankurai/sha256"
rollback_dir="${install_root}/rollback/jankurai"
install_lock_path="${install_root}/.jankurai-install.lock"
installer_pid="${BASHPID}"

[[ ! -e "${install_root}" || ( -d "${install_root}" && ! -L "${install_root}" ) ]] ||
  die "installation root is not a physical directory"
mkdir -p "${install_root}"
[[ -d "${install_root}" && ! -L "${install_root}" ]] ||
  die "installation root is not a physical directory"
[[ "$(realpath -e -- "${install_root}")" == "${install_root}" ]] ||
  die "installation root changed during creation"
install_root_uid="$(stat -Lc '%u' -- "${install_root}")"
install_root_gid="$(stat -Lc '%g' -- "${install_root}")"
install_root_mode="$(stat -Lc '%a' -- "${install_root}")"
[[ "${install_root_uid}" == "$(id -u)" && "${install_root_gid}" == "$(id -g)" ]] ||
  die "installation root is not owned by the current identity"
(( (8#${install_root_mode} & 8#022) == 0 )) ||
  die "installation root must not be group- or world-writable"

exec {install_root_fd}<"${install_root}"
install_root_fd_path="/proc/${installer_pid}/fd/${install_root_fd}"
install_root_identity="$(stat -Lc '%d:%i:%u:%g:%a' -- "${install_root_fd_path}")"
[[ "$(realpath -e -- "${install_root_fd_path}")" == "${install_root}" ]] ||
  die "installation root descriptor escaped physical custody"

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
flock -x "${install_lock_fd}" || die "unable to acquire exclusive installation lock"
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

token_file="${JERYU_FORGE_TOKEN_FILE:-/home/ubuntu/.jeryu/secrets/merge-token}"
[[ -r "${token_file}" ]] || die "local-forge credential is unavailable"
forge_token="$(tr -d '\n' < "${token_file}")"
[[ -n "${forge_token}" ]] || die "local-forge credential is empty"
git_bin=git
if [[ -n "${JERYU_INSTALL_TEST_GIT_BIN:-}" ]]; then
  [[ "${test_mode}" == "1" ]] || die "a Git test double is allowed only in explicit test mode"
  [[ -x "${JERYU_INSTALL_TEST_GIT_BIN}" ]] || die "Git test double is not executable"
  git_bin="${JERYU_INSTALL_TEST_GIT_BIN}"
fi
forge_git() {
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
manifest_commit="$(git -C "${manifest_root}" rev-parse HEAD)"
manifest_tree="$(git -C "${manifest_root}" rev-parse 'HEAD^{tree}')"
manifest_sha256="$(sha256_file "${manifest_root}/tool-manifest.toml")"
require_hex JERYU_TOOL_MANIFEST_COMMIT "${manifest_commit}" 40
require_hex JERYU_TOOL_MANIFEST_TREE "${manifest_tree}" 40
require_hex JERYU_TOOL_MANIFEST_SHA256 "${manifest_sha256}" 64
governance_status="diagnostic-candidate"
governance_protected_main=false
governance_protection="not-applicable"
if [[ "${test_mode}" != "1" ]]; then
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

require_transaction_custody
if [[ -e "${target_custody_path}" || -L "${target_custody_path}" ]]; then
  existing_target_fd=""
  open_custody_file "${install_dir_fd}" "${install_dir}" jankurai \
    existing_target_fd _ignored_file_identity ||
    die "existing target is not a single-link physical regular file"
  existing_target_descriptor="/proc/${installer_pid}/fd/${existing_target_fd}"
  existing_version="$("${existing_target_descriptor}" --version 2>/dev/null || true)"
  existing_sha="$(sha256_file "${existing_target_descriptor}")"
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
      printf 'jeryu jankurai already current: %s sha256=%s receipt=%s\n' \
        "${JANKURAI_VERSION}" "${existing_sha}" "${receipt}"
      exit 0
    fi
  fi
fi

scratch="$(mktemp -d /tmp/jeryu-install-jankurai.XXXXXX)"
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
installed_target_identity=""
success=0

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
  local status=$?
  trap - EXIT
  if [[ "${status}" -ne 0 && "${target_replaced}" -eq 1 && "${success}" -ne 1 ]]; then
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
  rm -rf "${scratch}"
  exit "${status}"
}
trap finish EXIT
trap 'exit 130' INT TERM HUP

actual_rustc="${JANKURAI_RUSTC_VERSION}"
actual_cargo="${JANKURAI_CARGO_VERSION}"
actual_target="${JANKURAI_TARGET_TRIPLE}"

candidate="${scratch}/out/bin/jankurai"
source_verification="release-authoritative"
if [[ "${test_mode}" == "1" ]]; then
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
  "${here}/build-jankurai-hermetic.sh" "${scratch}/source" "${candidate}"
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
jq -n -S \
  --arg schema "jeryu.jankurai-installation/v2" \
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
      network_scope:"local-forge-source-plus-closed-vendor-network-none",
      no_proxy:"127.0.0.1,localhost,::1"},
    governance:{status:$governance_status,manifest_repo:$manifest_repo,
      manifest_commit:$manifest_commit,manifest_tree:$manifest_tree,
      manifest_sha256:$manifest_sha,protected_main:$protected_main,
      protection_policy:$protection},
    binary:{sha256:$binary_sha,version_output:$version},
    installation:{path:$path,atomic:true,previous_binary_sha256:$previous_sha,
      rollback_artifact:$rollback_path,
      lock:{path:$install_lock_path,identity:$install_lock_identity,
        exclusive:true,held_through_receipt:true}},conclusion:"success"}' > "${receipt_stage}"
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
  receipt_install_leaf=""
  [[ "$(stat -Lc '%d:%i:%u:%g:%h' -- "${receipt_dir_fd_path}/${receipt_leaf}")" == \
     "${receipt_install_identity}" ]] || die "receipt publication identity changed"
  receipt_fd="${receipt_install_fd}"
  sync -f "${receipt_dir_fd_path}"
fi
[[ "$(sha256_file "/proc/${installer_pid}/fd/${receipt_fd}")" == "${receipt_sha}" ]] ||
  die "receipt content address mismatch"
require_transaction_custody

success=1
printf 'jeryu jankurai installed: %s sha256=%s path=%s receipt=%s\n' \
  "${installed_version}" "${installed_sha}" "${target}" "${receipt_path}"
