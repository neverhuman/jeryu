#!/usr/bin/env bash
# Sourced only by install-jankurai.sh. Do not execute.
[[ "${BASH_SOURCE[0]}" != "${0}" ]] || {
  printf '%s: source from install-jankurai.sh\n' "$(basename -- "${BASH_SOURCE[0]}")" >&2
  exit 1
}

remove_owned_scratch() {
  local directory="$1" identity="$2" mount_point links
  [[ -d "${directory}" && ! -L "${directory}" && -O "${directory}" &&
     "$(realpath -e -- "${directory}")" == "${directory}" &&
     "$(stat -c '%d:%i:%u' -- "${directory}")" == "${identity}" &&
     -r /proc/self/mountinfo ]] || return 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "${mount_point}"
    [[ "${mount_point}" != "${directory}" && "${mount_point}" != "${directory}/"* ]] || return 1
  done </proc/self/mountinfo
  # Refuse linked or mounted scratch rather than traversing a changed tree.
  links="$(find "${directory}" -xdev -type l -print -quit)" || return 1
  [[ -z "${links}" && ! -L "${directory}" &&
     "$(stat -c '%d:%i:%u' -- "${directory}")" == "${identity}" ]] || return 1
  rm -rf --one-file-system --preserve-root=all -- "${directory}"
}

candidate_finish() {
  local status=$?
  trap - EXIT
  if [[ -n "${candidate_state}" ]]; then
    remove_owned_scratch "${candidate_state}" "${candidate_state_identity}" || {
      printf 'install-jankurai: retained changed or mounted candidate inputs\n' >&2
      status=1
    }
  fi
  exit "${status}"
}

candidate_ancestors() {
  local directory="${install_root}" owner mode
  while [[ ! -e "${directory}" ]]; do directory="$(dirname -- "${directory}")"; done
  while :; do
    [[ -d "${directory}" && ! -L "${directory}" &&
       "$(realpath -e -- "${directory}")" == "${directory}" ]] || return 1
    owner="$(stat -c %u -- "${directory}")"
    mode="$(stat -c %a -- "${directory}")"
    [[ "${owner}" == 0 || "${owner}" == "$(id -u)" ]] || return 1
    (( (8#${mode} & 8#022) == 0 )) || return 1
    [[ "${directory}" != / ]] || break
    directory="$(dirname -- "${directory}")"
  done
}

public_git() {
  env -i PATH=/usr/bin:/bin HOME="${candidate_state}/home" \
    XDG_CONFIG_HOME="${candidate_state}/xdg" GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_COUNT=0 \
    GIT_TERMINAL_PROMPT=0 GIT_NO_REPLACE_OBJECTS=1 GIT_OPTIONAL_LOCKS=0 \
    GIT_TEMPLATE_DIR="${candidate_state}/git-template" \
    /usr/bin/git -C "${candidate_state}" -c credential.helper= -c core.askPass= -c core.hooksPath=/dev/null \
    -c core.fsmonitor=false -c http.extraHeader= -c http.followRedirects=false \
    -c http.sslVerify=true -c http.proxy= -c protocol.allow=never -c protocol.https.allow=always "$@"
}

candidate_renderer() {
  # Compilation is offline; the parent builds this exact control target first.
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    GIT_CONFIG_COUNT=0 GIT_TERMINAL_PROMPT=0 GIT_NO_REPLACE_OBJECTS=1 \
    cargo run --quiet --locked --offline \
    --manifest-path "${manifest_root}/crates/jeryu-tool-control/Cargo.toml" \
    --bin jeryu-toolctl -- --tool-root "${manifest_root}" \
    render-monorepo-candidate --monorepo-root "${candidate_monorepo}" \
    --check --expected-head "${expected_head}"
}

candidate_hold_input() {
  local relative="$1" held="$2" output_blob="$3" output_sha="$4" blob sha
  [[ -f "${candidate_monorepo}/${relative}" && ! -L "${candidate_monorepo}/${relative}" &&
    "$(realpath -e -- "${candidate_monorepo}/${relative}")" == "${candidate_monorepo}/${relative}" ]] ||
    die "candidate input is not a physical committed file"
  blob="$(public_git -C "${candidate_monorepo}" rev-parse "${expected_head}:${relative}")"
  require_hex candidate-input-blob "${blob}" 40
  [[ "$(public_git -C "${candidate_monorepo}" cat-file -t "${blob}")" == blob ]] ||
    die "candidate input is not a Git blob"
  public_git -C "${candidate_monorepo}" cat-file blob "${blob}" >"${held}"
  chmod 400 "${held}"
  sha="$(sha256_file "${held}")"
  [[ "$(sha256_file "${candidate_monorepo}/${relative}")" == "${sha}" ]] ||
    die "candidate input differs from its committed blob"
  printf -v "${output_blob}" '%s' "${blob}"
  printf -v "${output_sha}" '%s' "${sha}"
}

candidate_recheck() {
  local relative held expected_sha
  [[ "${public_candidate}" == 1 ]] || return 0
  candidate_ancestors || die "candidate install ancestors changed"
  [[ -d "${candidate_state}" && ! -L "${candidate_state}" && -O "${candidate_state}" &&
     "$(realpath -e -- "${candidate_state}")" == "${candidate_state}" &&
     "$(stat -c '%d:%i:%u' -- "${candidate_state}")" == "${candidate_state_identity}" &&
     "$(stat -c %a -- "${candidate_state}")" == 700 ]] ||
    die "candidate retained input custody changed"
  [[ "$(public_git -C "${candidate_monorepo}" rev-parse HEAD)" == "${expected_head}" &&
     "$(public_git -C "${candidate_monorepo}" rev-parse 'HEAD^{tree}')" == "${manifest_tree}" &&
     -z "$(public_git -C "${candidate_monorepo}" status --porcelain --untracked-files=all)" ]] ||
    die "candidate checkout is no longer the exact clean source"
  for relative in "${candidate_pin_path}" "${candidate_builder_path}" "${candidate_predicate_path}"; do
    case "${relative}" in
      "${candidate_pin_path}") held=pin.env; expected_sha="${candidate_pin_sha}" ;;
      "${candidate_builder_path}") held=builder.sh; expected_sha="${candidate_builder_sha}" ;;
      *) held=receipt.jq; expected_sha="${candidate_predicate_sha}" ;;
    esac
    [[ -f "${candidate_state}/${held}" && ! -L "${candidate_state}/${held}" &&
       "$(stat -c '%a:%h' -- "${candidate_state}/${held}")" == 400:1 &&
       -f "${candidate_monorepo}/${relative}" && ! -L "${candidate_monorepo}/${relative}" &&
       "$(realpath -e -- "${candidate_monorepo}/${relative}")" == "${candidate_monorepo}/${relative}" &&
       "$(sha256_file "${candidate_state}/${held}")" == "${expected_sha}" &&
       "$(sha256_file "${candidate_monorepo}/${relative}")" == "${expected_sha}" ]] ||
      die "candidate committed or retained input changed"
  done
  [[ -f "${candidate_state}/renderer.json" && ! -L "${candidate_state}/renderer.json" &&
     "$(stat -c '%a:%h' -- "${candidate_state}/renderer.json")" == 400:1 &&
     "$(sha256_file "${candidate_state}/renderer.json")" == "${candidate_renderer_sha}" ]] ||
    die "retained renderer metadata changed"
  candidate_renderer >"${candidate_state}/renderer-recheck.json"
  [[ "$(sha256_file "${candidate_state}/renderer-recheck.json")" == "${candidate_renderer_sha}" ]] ||
    die "candidate renderer identity or committed consumer bytes changed"
}

candidate_receipt_valid() {
  local receipt="$1"
  jq -e --argjson renderer "$(cat "${candidate_state}/renderer.json")" \
    --arg renderer_sha "${candidate_renderer_sha}" --argjson pin "${candidate_pin_json}" \
    --arg pin_blob "${candidate_pin_blob}" --arg pin_sha "${candidate_pin_sha}" \
    --arg builder_blob "${candidate_builder_blob}" --arg builder_sha "${candidate_builder_sha}" \
    --arg binary_path "${target}" --arg install_root "${install_root}" \
    --arg lock_identity "${install_lock_identity}" \
    -f "${candidate_state}/receipt.jq" "${receipt}" >/dev/null || return 1
  [[ "$(jq -S . "${receipt}" | sha256sum | awk '{print $1}')" == "$(sha256_file "${receipt}")" ]]
}

candidate_receipt_custody() {
  local fd="$1" digest="$2" path="$3"
  local held="/proc/${installer_pid}/fd/${fd}"
  [[ -f "${path}" && ! -L "${path}" &&
     "$(realpath -e -- "${path}")" == "${path}" &&
     "$(realpath -e -- "${held}")" == "${path}" &&
     "$(stat -Lc '%a:%h' -- "${held}")" == 600:1 &&
     "$(stat -Lc '%d:%i:%u:%g:%a:%h' -- "${held}")" == \
       "$(stat -Lc '%d:%i:%u:%g:%a:%h' -- "${path}")" &&
     "$(sha256_file "${held}")" == "${digest}" ]]
}

# The generated, retained pin supplies JANKURAI_REV before this function runs.
# shellcheck disable=SC2153
candidate_public_readback() {
  local remote_head
  remote_head="$(public_git ls-remote --tags "${JANKURAI_REPO}" \
    "refs/tags/${JANKURAI_TAG}" "refs/tags/${JANKURAI_TAG}^{}" |
    awk -v direct="refs/tags/${JANKURAI_TAG}" -v peeled="refs/tags/${JANKURAI_TAG}^{}" '
      $2 == peeled {print $1; found=1; exit}
      $2 == direct {direct_rev=$1}
      END {if (!found && direct_rev != "") print direct_rev}')"
  [[ "${remote_head}" == "${JANKURAI_REV}" ]] || die "public immutable tag readback mismatch"
}

candidate_prepare_build_cache() {
  # Fetch only the exact lock's public crates.io inventory outside the offline build.
  awk '/^source = / && $0 != "source = \"registry+https://github.com/rust-lang/crates.io-index\"" {exit 1}' \
    "${scratch}/source/Cargo.lock" || die "candidate lock contains a non-public-registry source"
  mkdir "${candidate_state}/cargo-home" "${candidate_state}/docker-config"
  candidate_cargo="$(command -v cargo)"
  [[ "${candidate_cargo}" == /* && -x "${candidate_cargo}" ]] || die "Cargo executable unavailable"
  candidate_cargo_dir="$(dirname -- "${candidate_cargo}")"
  candidate_rustup_home="${RUSTUP_HOME:-${HOME}/.rustup}"
  [[ -d "${candidate_rustup_home}" && ! -L "${candidate_rustup_home}" &&
    "$(realpath -e -- "${candidate_rustup_home}")" == "${candidate_rustup_home}" ]] ||
    die "Rustup home is not a physical directory"
  (
    cd "${candidate_state}"
    env -i PATH="${candidate_cargo_dir}:/usr/bin:/bin" HOME="${candidate_state}/home" \
      CARGO_HOME="${candidate_state}/cargo-home" RUSTUP_HOME="${candidate_rustup_home}" \
      CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse CARGO_NET_OFFLINE=false \
      "${candidate_cargo}" "+${JANKURAI_RUST_TOOLCHAIN}" fetch --locked \
      --manifest-path "${scratch}/source/Cargo.toml"
  )
  [[ -S /run/docker.sock && ! -L /run/docker.sock &&
    "$(realpath -e /run/docker.sock)" == /run/docker.sock &&
    "$(stat -c %u /run/docker.sock)" == 0 &&
    "$(stat -c %u /run)" == 0 && -f /usr/bin/docker && ! -L /usr/bin/docker &&
    -x /usr/bin/docker && "$(stat -c '%a:%u:%g:%h' /usr/bin/docker)" == 755:0:0:1 ]] ||
    die "candidate builder preparation requires the physical local Docker endpoint"
  (( (8#$(stat -c %a /run/docker.sock) & 8#002) == 0 &&
     (8#$(stat -c %a /run) & 8#022) == 0 )) || die "Docker endpoint is writable by another user"
  env -i PATH=/usr/bin:/bin /usr/bin/docker --host unix:///run/docker.sock \
    --config "${candidate_state}/docker-config" pull "${JANKURAI_BUILDER_IMAGE}"
}

install_jankurai_load_pin_and_root() {
  default_pin_env="${here}/../generated/jankurai-pin.env"
  test_mode="${JERYU_INSTALL_TEST_MODE:-0}"
  [[ "${test_mode}" == "0" || "${test_mode}" == "1" ]] || die "test mode must be 0 or 1"

  governed_install_root="/home/ubuntu/.jeryu"
  if [[ "${public_candidate}" == 1 ]]; then
    install_root_input="${JERYU_INSTALL_ROOT:-${XDG_CACHE_HOME:-${HOME}/.cache}/jeryu/ci-tools/jankurai}"
  else
    install_root_input="${JERYU_INSTALL_ROOT:-${governed_install_root}}"
  fi
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
  if [[ "${test_mode}" != "1" && "${public_candidate}" != "1" && "${install_root}" != "${governed_install_root}" ]]; then
    die "governed installation root must be ${governed_install_root}"
  fi

  pin_env="${JERYU_PIN_ENV:-${default_pin_env}}"
  if [[ "${public_candidate}" == 1 ]]; then
    case "${install_root}" in
      /|/home/ubuntu/.jeryu|/home/ubuntu/.jeryu/*|/opt/jain-ci|/opt/jain-ci/*)
        die "public candidate mode must not target a governed installation" ;;
    esac
    candidate_ancestors || die "candidate installation ancestors are not physical and protected"
    manifest_root="$(realpath -e -- "${here}/..")"
    candidate_monorepo="$(realpath -e -- "${manifest_root}/../..")"
    [[ "${manifest_root}" == "${candidate_monorepo}/components/jeryu-tool" &&
      -d "${candidate_monorepo}/.git" && ! -L "${candidate_monorepo}/.git" ]] ||
      die "public candidate requires an ordinary physical monorepo checkout"
    candidate_state="$(mktemp -d /tmp/jeryu-public-candidate.XXXXXX)"
    candidate_state_identity="$(stat -c '%d:%i:%u' -- "${candidate_state}")"
    trap candidate_finish EXIT
    trap 'exit 130' INT TERM HUP
    mkdir "${candidate_state}/home" "${candidate_state}/xdg" "${candidate_state}/git-template"
    [[ "$(public_git -C "${candidate_monorepo}" rev-parse HEAD)" == "${expected_head}" &&
      -z "$(public_git -C "${candidate_monorepo}" status --porcelain --untracked-files=all)" ]] ||
      die "public candidate requires the exact clean committed head"
    manifest_tree="$(public_git -C "${candidate_monorepo}" rev-parse 'HEAD^{tree}')"
    candidate_renderer >"${candidate_state}/renderer.json"
    chmod 400 "${candidate_state}/renderer.json"
    candidate_renderer_sha="$(sha256_file "${candidate_state}/renderer.json")"
    jq -e --arg head "${expected_head}" --arg tree "${manifest_tree}" '
      .schema == "jeryu.jankurai-candidate-render/v1" and .mode == "check" and .drift == false and
      .scope == {complete:true,repositories:["jeryu","jeryu-cache","jeryu-ci-runner","jeryu-core",
        "jeryu-deploy","jeryu-intelligence","jeryu-jira","jeryu-release-ops","jeryu-tool",
        "jeryu-tool-finder","jeryu-web"]} and
      .source == {repository:"https://github.com/neverhuman/jeryu.git",commit:$head,tree:$tree,clean_at_start:true} and
      .manifest.path == "components/jeryu-tool/tool-manifest.toml" and
      .distribution.repository == "https://github.com/neverhuman/jankurai.git" and
      .governance == {protected_main:false,handover:"pending",predecessor_authentication:"not-performed"} and
      .verification == {build:"not-performed",installation:"not-performed",public_readback:"not-performed"} and
      (.consumers | length > 0 and all(.[]; .changed == false and .before_sha256 == .expected_sha256))
    ' "${candidate_state}/renderer.json" >/dev/null || die "candidate renderer metadata is not exact and drift-free"
    candidate_pin_path="components/jeryu-tool/generated/jankurai-pin.env"
    candidate_builder_path="components/jeryu-tool/ops/build-jankurai-hermetic.sh"
    candidate_predicate_path="components/jeryu-tool/ops/public-candidate-receipt.jq"
    candidate_hold_input "${candidate_pin_path}" "${candidate_state}/pin.env" candidate_pin_blob candidate_pin_sha
    candidate_hold_input "${candidate_builder_path}" "${candidate_state}/builder.sh" candidate_builder_blob candidate_builder_sha
    candidate_hold_input "${candidate_predicate_path}" "${candidate_state}/receipt.jq" candidate_predicate_blob candidate_predicate_sha
    [[ "$(public_git -C "${candidate_monorepo}" hash-object "${candidate_state}/receipt.jq")" == "${candidate_predicate_blob}" ]] ||
      die "retained receipt predicate differs from its committed blob"
    candidate_hold_input components/jeryu-tool/tool-manifest.toml "${candidate_state}/manifest.toml" candidate_manifest_blob candidate_manifest_sha
    jq -e --arg pin "${candidate_pin_sha}" --arg manifest "${candidate_manifest_sha}" \
      --arg blob "${candidate_manifest_blob}" '
      .generated_pin_sha256 == $pin and .manifest.sha256 == $manifest and .manifest.blob == $blob
    ' "${candidate_state}/renderer.json" >/dev/null || die "candidate renderer inputs differ from retained Git bytes"
    pin_env="${candidate_state}/pin.env"
  fi
  if [[ "${pin_env}" != "${default_pin_env}" && "${test_mode}" != "1" && "${public_candidate}" != "1" ]]; then
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
  [[ "${JANKURAI_REPO}" == "https://github.com/neverhuman/jankurai.git" ]] ||
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

  if [[ "${public_candidate}" == 1 ]]; then
    candidate_pin_json="$(for name in "${required_pin_vars[@]}"; do printf '%s\0%s\0' "${name}" "${!name}"; done |
      jq -Rs 'split("\u0000")[:-1] | [range(0; length; 2) as $i | {key:.[$i],value:.[$i+1]}] | from_entries')"
    jq -e --arg producer "${JANKURAI_REPO}" --arg tag "${JANKURAI_TAG}" \
      --arg commit "${JANKURAI_REV}" --arg tree "${JANKURAI_SOURCE_TREE}" '
      .producer_repository == $producer and .distribution.tag == $tag and
      .distribution.commit == $commit and .distribution.tree == $tree
    ' "${candidate_state}/renderer.json" >/dev/null || die "public distribution changed the producer identity"
    JANKURAI_REPO="$(jq -er .distribution.repository "${candidate_state}/renderer.json")"
  fi

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

}
