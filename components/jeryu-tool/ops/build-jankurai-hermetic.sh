#!/usr/bin/env bash
# Reproduce the governed Jankurai binary from exact source and a closed vendor
# inventory inside one digest-pinned, network-disabled OCI builder.
set -euo pipefail
umask 077

die() {
  printf 'build-jankurai-hermetic: %s\n' "$*" >&2
  exit 1
}

sha256_file() {
  sha256sum "$1" | awk '{print $1}'
}

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
pin_env="${JERYU_PIN_ENV:-${here}/../generated/jankurai-pin.env}"
source_root="${1:-}"
output="${2:-}"
[[ "${source_root}" == /* && "${output}" == /* ]] ||
  die "usage: $0 ABSOLUTE_SOURCE_ROOT ABSOLUTE_OUTPUT"
[[ -r "${pin_env}" ]] || die "generated pin is missing: ${pin_env}"
# shellcheck source=/dev/null
source "${pin_env}"

required=(
  JANKURAI_REV JANKURAI_VERSION JANKURAI_SOURCE_TREE
  JANKURAI_SOURCE_ARCHIVE_SHA256 JANKURAI_CARGO_LOCK_SHA256
  JANKURAI_BINARY_SHA256 JANKURAI_RUST_TOOLCHAIN JANKURAI_RUSTC_VERSION
  JANKURAI_CARGO_VERSION JANKURAI_TARGET_TRIPLE JANKURAI_BUILD_MODE
  JANKURAI_PACKAGE_PATH JANKURAI_BUILDER_IMAGE JANKURAI_BUILDER_IMAGE_ID
  JANKURAI_LINKER_VERSION JANKURAI_GLIBC_VERSION
  JANKURAI_VENDOR_FILES_SHA256 JANKURAI_VENDOR_FILE_COUNT
  JANKURAI_CARGO_CONFIG_SHA256 JANKURAI_BUILD_ENVIRONMENT
  JANKURAI_RUSTFLAGS JANKURAI_BUILD_COMMAND JANKURAI_BUILD_CONTEXT_SHA256
)
for name in "${required[@]}"; do
  [[ -n "${!name:-}" ]] || die "generated pin is missing ${name}"
done
[[ "${JANKURAI_BUILD_MODE}" == "oci-vendor-locked-offline-workspace-member-v2" ]] ||
  die "unsupported build mode: ${JANKURAI_BUILD_MODE}"
[[ "${JANKURAI_PACKAGE_PATH}" == "crates/jankurai" ]] ||
  die "unsupported package path: ${JANKURAI_PACKAGE_PATH}"

source_root="$(realpath -m "${source_root}")"
[[ -d "${source_root}" && ! -L "${source_root}" ]] ||
  die "source root is not one physical directory"
[[ "$(git -C "${source_root}" rev-parse HEAD)" == "${JANKURAI_REV}" ]] ||
  die "source commit mismatch"
[[ "$(git -C "${source_root}" rev-parse 'HEAD^{tree}')" == "${JANKURAI_SOURCE_TREE}" ]] ||
  die "source tree mismatch"
[[ -z "$(git -C "${source_root}" status --porcelain --untracked-files=all)" ]] ||
  die "source checkout is dirty before build"
source_archive_sha="$(
  git -C "${source_root}" archive --format=tar HEAD | sha256sum | awk '{print $1}'
)"
[[ "${source_archive_sha}" == "${JANKURAI_SOURCE_ARCHIVE_SHA256}" ]] ||
  die "source archive mismatch"
[[ "$(sha256_file "${source_root}/Cargo.lock")" == "${JANKURAI_CARGO_LOCK_SHA256}" ]] ||
  die "Cargo.lock mismatch"

output_parent="$(dirname "${output}")"
[[ -d "${output_parent}" && ! -L "${output_parent}" ]] ||
  die "output parent is not one physical directory"
output_parent="$(cd -P "${output_parent}" && pwd)"
output="${output_parent}/$(basename "${output}")"
[[ ! -e "${output}" ]] || die "output already exists: ${output}"

actual_cargo="$(cargo "+${JANKURAI_RUST_TOOLCHAIN}" --version)"
[[ "${actual_cargo}" == "${JANKURAI_CARGO_VERSION}" ]] ||
  die "vendor generator mismatch: got ${actual_cargo}"

scratch="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-jankurai-build.XXXXXX")"
cleanup() {
  chmod -R u+rwX "${scratch}" 2>/dev/null || true
  rm -rf -- "${scratch}"
}
trap cleanup EXIT
mkdir -p "${scratch}/vendor" "${scratch}/target" "${scratch}/out"

if ! CARGO_NET_OFFLINE=true GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
  cargo "+${JANKURAI_RUST_TOOLCHAIN}" vendor --locked --offline --versioned-dirs \
  --manifest-path "${source_root}/Cargo.toml" "${scratch}/vendor" \
  >"${scratch}/vendor-config.raw" 2>"${scratch}/vendor.log"; then
  tail -n 20 "${scratch}/vendor.log" >&2
  die "closed vendor materialization failed offline"
fi

docker_bin="/usr/bin/docker"
[[ -f "${docker_bin}" && ! -L "${docker_bin}" && -x "${docker_bin}" ]] ||
  die "container engine is not the governed /usr/bin/docker"
[[ "$(stat -c '%a:%u:%g:%h' "${docker_bin}")" == "755:0:0:1" ]] ||
  die "container engine custody mismatch"
actual_image_id="$("${docker_bin}" image inspect --format '{{.Id}}' \
  "${JANKURAI_BUILDER_IMAGE}")" || die "pinned builder image is unavailable"
[[ "${actual_image_id}" == "${JANKURAI_BUILDER_IMAGE_ID}" ]] ||
  die "builder image ID mismatch"
"${docker_bin}" image inspect --format '{{range .RepoDigests}}{{println .}}{{end}}' \
  "${JANKURAI_BUILDER_IMAGE}" | grep -Fx "${JANKURAI_BUILDER_IMAGE}" >/dev/null ||
  die "builder image repository digest mismatch"

sed 's#^directory = ".*"$#directory = "/opt/jeryu/vendor"#' \
  "${scratch}/vendor-config.raw" >"${scratch}/cargo-config.toml"
printf '\n[net]\noffline = true\n' >>"${scratch}/cargo-config.toml"
grep -F "${scratch}" "${scratch}/cargo-config.toml" >/dev/null &&
  die "Cargo configuration leaked a temporary source path"
[[ -z "$(find "${scratch}/vendor" \
  \( -type l -o \( ! -type d ! -type f \) \) -print -quit)" ]] ||
  die "vendor closure contains a symlink or special node"
(
  cd "${scratch}"
  find vendor -type f -print0 | LC_ALL=C sort -z |
    xargs -0 sha256sum >vendor-files.sha256
)
vendor_count="$(wc -l <"${scratch}/vendor-files.sha256" | tr -d ' ')"
[[ "${vendor_count}" == "${JANKURAI_VENDOR_FILE_COUNT}" ]] ||
  die "vendor file count mismatch: got ${vendor_count}"
vendor_inventory_sha="$(sha256_file "${scratch}/vendor-files.sha256")"
[[ "${vendor_inventory_sha}" == "${JANKURAI_VENDOR_FILES_SHA256}" ]] ||
  die "vendor inventory mismatch"
(
  cd "${scratch}"
  sha256sum --check --strict vendor-files.sha256 >/dev/null
)
cargo_config_sha="$(sha256_file "${scratch}/cargo-config.toml")"
[[ "${cargo_config_sha}" == "${JANKURAI_CARGO_CONFIG_SHA256}" ]] ||
  die "Cargo configuration mismatch"

context_text() {
  printf '%s\n' \
    "schema=jeryu.jankurai-build-context/v1" \
    "source_archive_sha256=${JANKURAI_SOURCE_ARCHIVE_SHA256}" \
    "cargo_lock_sha256=${JANKURAI_CARGO_LOCK_SHA256}" \
    "vendor_files_sha256=${JANKURAI_VENDOR_FILES_SHA256}" \
    "vendor_file_count=${JANKURAI_VENDOR_FILE_COUNT}" \
    "cargo_config_sha256=${JANKURAI_CARGO_CONFIG_SHA256}" \
    "builder_image=${JANKURAI_BUILDER_IMAGE}" \
    "builder_image_id=${JANKURAI_BUILDER_IMAGE_ID}" \
    "rustc_version=${JANKURAI_RUSTC_VERSION}" \
    "cargo_version=${JANKURAI_CARGO_VERSION}" \
    "target_triple=${JANKURAI_TARGET_TRIPLE}" \
    "linker_version=${JANKURAI_LINKER_VERSION}" \
    "glibc_version=${JANKURAI_GLIBC_VERSION}" \
    "build_mode=${JANKURAI_BUILD_MODE}" \
    "package_path=${JANKURAI_PACKAGE_PATH}" \
    "build_environment=${JANKURAI_BUILD_ENVIRONMENT}" \
    "rustflags=${JANKURAI_RUSTFLAGS}" \
    "build_command=${JANKURAI_BUILD_COMMAND}"
}
context_sha="$(context_text | sha256sum | awk '{print $1}')"
[[ "${context_sha}" == "${JANKURAI_BUILD_CONTEXT_SHA256}" ]] ||
  die "build context identity mismatch"

build_uid="$(id -u)"
build_gid="$(id -g)"
[[ "${build_uid}" != "0" ]] || die "the hermetic builder may not execute as root"
chmod -R a+rX "${source_root}" "${scratch}/vendor" "${scratch}/cargo-config.toml"

"${docker_bin}" run --rm --pull=never --user "${build_uid}:${build_gid}" \
  --network none --read-only --cap-drop ALL --security-opt no-new-privileges \
  --pids-limit 1024 --memory 6g --cpus 8 \
  --tmpfs "/tmp:rw,nosuid,nodev,noexec,size=64m,uid=${build_uid},gid=${build_gid},mode=700" \
  --mount "type=bind,src=${source_root},dst=/opt/jeryu/jankurai,readonly" \
  --mount "type=bind,src=${scratch}/vendor,dst=/opt/jeryu/vendor,readonly" \
  --mount "type=bind,src=${scratch}/cargo-config.toml,dst=/usr/local/cargo/config.toml,readonly" \
  --mount "type=bind,src=${scratch}/target,dst=/opt/jeryu/target" \
  --mount "type=bind,src=${scratch}/out,dst=/opt/jeryu/out" \
  --env HOME=/tmp --env CARGO_TARGET_DIR=/opt/jeryu/target \
  --env CARGO_NET_OFFLINE=true --env SOURCE_DATE_EPOCH=0 \
  --env TZ=UTC --env LC_ALL=C --env LANG=C \
  --env "RUSTFLAGS=${JANKURAI_RUSTFLAGS}" \
  --env "EXPECTED_RUSTC=${JANKURAI_RUSTC_VERSION}" \
  --env "EXPECTED_CARGO=${JANKURAI_CARGO_VERSION}" \
  --env "EXPECTED_TARGET=${JANKURAI_TARGET_TRIPLE}" \
  --env "EXPECTED_LINKER=${JANKURAI_LINKER_VERSION}" \
  --env "EXPECTED_GLIBC=${JANKURAI_GLIBC_VERSION}" \
  --env "EXPECTED_VERSION=${JANKURAI_VERSION}" \
  --env "EXPECTED_BINARY_SHA256=${JANKURAI_BINARY_SHA256}" \
  "${JANKURAI_BUILDER_IMAGE}" sh -eu -c '
    test "$(rustc --version)" = "${EXPECTED_RUSTC}"
    test "$(cargo --version)" = "${EXPECTED_CARGO}"
    test "$(rustc -vV | awk "/^host:/ {print \$2}")" = "${EXPECTED_TARGET}"
    test "$(ld --version | head -n1)" = "${EXPECTED_LINKER}"
    test "$(ldd --version | head -n1)" = "${EXPECTED_GLIBC}"
    cargo install --locked --offline --path /opt/jeryu/jankurai/crates/jankurai \
      --root /opt/jeryu/out --bin jankurai
    test "$(/opt/jeryu/out/bin/jankurai --version)" = "${EXPECTED_VERSION}"
    printf "%s  %s\n" "${EXPECTED_BINARY_SHA256}" \
      /opt/jeryu/out/bin/jankurai | sha256sum --check --strict
  '

candidate="${scratch}/out/bin/jankurai"
[[ -f "${candidate}" && ! -L "${candidate}" && -x "${candidate}" ]] ||
  die "builder did not produce one executable"
[[ "$(sha256_file "${candidate}")" == "${JANKURAI_BINARY_SHA256}" ]] ||
  die "built digest mismatch"
[[ "$("${candidate}" --version 2>/dev/null)" == "${JANKURAI_VERSION}" ]] ||
  die "built version mismatch"
[[ -z "$(git -C "${source_root}" status --porcelain --untracked-files=all)" ]] ||
  die "source checkout became dirty during build"

stage="${output_parent}/.$(basename "${output}").stage.$$"
cp "${candidate}" "${stage}"
chmod 755 "${stage}"
[[ "$(sha256_file "${stage}")" == "${JANKURAI_BINARY_SHA256}" ]] ||
  die "staged binary digest mismatch"
mv "${stage}" "${output}"
printf 'hermetic Jankurai build: %s sha256=%s context=%s\n' \
  "${JANKURAI_VERSION}" "${JANKURAI_BINARY_SHA256}" \
  "${JANKURAI_BUILD_CONTEXT_SHA256}"
