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

build_uid="$(id -u)"
build_gid="$(id -g)"
[[ "${build_uid}" != "0" ]] || die "the hermetic builder may not execute as root"

require_owned_directory() {
  local path="$1" role="$2"
  [[ "${path}" == /* && -d "${path}" && ! -L "${path}" && -O "${path}" ]] ||
    die "${role} is not one caller-owned physical directory"
  [[ "$(realpath -e -- "${path}")" == "${path}" ]] ||
    die "${role} contains a path alias"
}
require_owned_directory "${source_root}" "source root"
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
require_owned_directory "${output_parent}" "output parent"
[[ "${output}" == "$(realpath -ms -- "${output}")" ]] || die "output contains a path alias"
[[ ! -e "${output}" && ! -L "${output}" ]] || die "output already exists: ${output}"
[[ $((8#$(stat -c %a -- "${output_parent}") & 0022)) == 0 ]] ||
  die "output parent is writable by another user"

# Cargo uses available_parallelism when no job override is supplied. Restrict
# affinity as well as quota, keeping the frozen command and environment intact.
build_cpus="$(awk '/^Cpus_allowed_list:/ {
  count = split($2, ranges, ",")
  for (i = 1; i <= count && chosen < 2; i++) {
    split(ranges[i], ends, "-")
    last = (ends[2] == "" ? ends[1] : ends[2])
    for (cpu = ends[1] + 0; cpu <= last && chosen < 2; cpu++) {
      printf "%s%d", (chosen++ ? "," : ""), cpu
    }
  }
  print ""
  if (chosen != 2) exit 1
}' /proc/self/status)" || die "two CPUs are required for the hermetic build"
[[ "${build_cpus}" =~ ^[0-9]+,[0-9]+$ ]] || die "cannot determine two allowed CPUs"
printf 'hermetic build resources: cpuset=%s cpus=2 memory=6g\n' "${build_cpus}"

actual_cargo="$(cargo "+${JANKURAI_RUST_TOOLCHAIN}" --version)"
[[ "${actual_cargo}" == "${JANKURAI_CARGO_VERSION}" ]] ||
  die "vendor generator mismatch: got ${actual_cargo}"

tmp_parent="${TMPDIR:-/tmp}"
[[ "${tmp_parent}" == /* && -d "${tmp_parent}" && ! -L "${tmp_parent}" &&
  "$(realpath -e -- "${tmp_parent}")" == "${tmp_parent}" ]] ||
  die "scratch parent is not one physical absolute directory"
scratch="$(mktemp -d "${tmp_parent}/jeryu-jankurai-build.XXXXXX")"
scratch_identity="$(stat -c '%d:%i:%u' -- "${scratch}")"
stage=""
stage_identity=""
create_attempted=0
container_removed=0
container_id=""
docker_call_limit=5
scratch_is_safe() {
  local mount_point links
  [[ -d "${scratch}" && ! -L "${scratch}" && -O "${scratch}" &&
    "$(realpath -e -- "${scratch}")" == "${scratch}" &&
    "$(stat -c '%d:%i:%u' -- "${scratch}")" == "${scratch_identity}" &&
    -r /proc/self/mountinfo ]] || return 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "${mount_point}"
    [[ "${mount_point}" != "${scratch}" && "${mount_point}" != "${scratch}/"* ]] || return 1
  done </proc/self/mountinfo
  links="$(find "${scratch}" -xdev -type l -print -quit)" || return 1
  [[ -z "${links}" ]]
}
cleanup() {
  local status=$?
  trap - EXIT
  trap '' INT TERM HUP
  if (( create_attempted )) && ! container_cleanup; then
    printf 'retaining builder source/scratch: container closure unknown; control=%s name=%s\n' \
      "${control}" "${container_name}" >&2
    exit 1
  fi
  if [[ -n "${stage}" ]]; then
    if [[ -f "${stage}" && ! -L "${stage}" && -O "${stage}" &&
      "$(stat -c '%d:%i:%u' -- "${stage}")" == "${stage_identity}" ]]; then
      rm -f -- "${stage}" || status=1
    else
      printf 'retaining changed build stage: %s\n' "${stage}" >&2
      status=1
    fi
  fi
  if scratch_is_safe; then
    rm -rf --one-file-system --preserve-root=all -- "${scratch}" || status=1
  else
    printf 'retaining changed, mounted, or linked build scratch: %s\n' "${scratch}" >&2
    status=1
  fi
  exit "${status}"
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP
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
docker_socket="/run/docker.sock"
[[ -S "${docker_socket}" && ! -L "${docker_socket}" &&
  "$(realpath -e -- "${docker_socket}")" == "${docker_socket}" &&
  "$(stat -c '%u:%h' -- "${docker_socket}")" == "0:1" &&
  "$(stat -c %u -- /run)" == 0 ]] || die "local Docker socket custody mismatch"
[[ $((8#$(stat -c %a -- "${docker_socket}") & 0002)) == 0 &&
  $((8#$(stat -c %a -- /run) & 0022)) == 0 ]] ||
  die "local Docker socket or parent is writable by another user"
docker_socket_identity="$(stat -c '%d:%i:%u:%g:%a:%h' -- "${docker_socket}")"
mkdir "${scratch}/docker-config"
local_docker() {
  [[ -S "${docker_socket}" && ! -L "${docker_socket}" &&
    "$(stat -c '%d:%i:%u:%g:%a:%h' -- "${docker_socket}")" == "${docker_socket_identity}" ]] ||
    die "local Docker socket changed after admission"
  env -i PATH=/usr/bin:/bin /usr/bin/timeout --foreground --signal=TERM --kill-after=2s \
    "${docker_call_limit:-5}s" "${docker_bin}" \
    --host "unix://${docker_socket}" --config "${scratch}/docker-config" "$@"
}
actual_image_id="$(local_docker image inspect --format '{{.Id}}' \
  "${JANKURAI_BUILDER_IMAGE}")" || die "pinned builder image is unavailable"
[[ "${actual_image_id}" == "${JANKURAI_BUILDER_IMAGE_ID}" ]] ||
  die "builder image ID mismatch"
local_docker image inspect --format '{{range .RepoDigests}}{{println .}}{{end}}' \
  "${JANKURAI_BUILDER_IMAGE}" | grep -Fx "${JANKURAI_BUILDER_IMAGE}" >/dev/null ||
  die "builder image repository digest mismatch"

sed 's#^directory = ".*"$#directory = "/opt/jeryu/vendor"#' \
  "${scratch}/vendor-config.raw" >"${scratch}/cargo-config.toml"
printf '\n[net]\noffline = true\n' >>"${scratch}/cargo-config.toml"
grep -F "${scratch}" "${scratch}/cargo-config.toml" >/dev/null &&
  die "Cargo configuration leaked a scratch source path"
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

# The control directory is outside all five bind mounts and never guest writable.
container_control_valid() {
  [[ -d "${control}" && ! -L "${control}" &&
     "$(realpath -e -- "${control}")" == "${control}" &&
     "$(stat -c '%d:%i:%u:%a' -- "${control}")" == "${control_identity}" &&
     "$(stat -c '%d:%i:%u' -- "${scratch}")" == "${scratch_identity}" ]]
}

container_read_id() {
  local fd identity held value size
  container_control_valid || return 1
  [[ -f "${control}/cid" && ! -L "${control}/cid" && -O "${control}/cid" &&
     "$(stat -c '%a:%h' -- "${control}/cid")" == 600:1 ]] || return 1
  size="$(stat -c %s -- "${control}/cid")" || return 1
  [[ "${size}" == 64 || "${size}" == 65 ]] || return 1
  exec {fd}<"${control}/cid" || return 1
  held="/proc/${BASHPID}/fd/${fd}"
  identity="$(stat -Lc '%d:%i:%u:%a:%h:%s:%y:%z' -- "${held}")" || return 1
  value="$(cat "${held}")" || return 1
  [[ "${value}" =~ ^[0-9a-f]{64}$ && ! -L "${control}/cid" &&
     "$(stat -c '%d:%i:%u:%a:%h:%s:%y:%z' -- "${control}/cid")" == "${identity}" &&
     "$(stat -Lc '%d:%i:%u:%a:%h:%s:%y:%z' -- "${held}")" == "${identity}" ]] || return 1
  exec {fd}<&-
  [[ -z "${container_id}" || "${container_id}" == "${value}" ]] || return 1
  container_id="${value}"
}

container_inspect() {
  container_read_id || return 1
  local_docker container inspect --format '{{json .}}' "${container_id}" \
    >"${control}/inspect.json" 2>"${control}/inspect.stderr" || return 1
  jq -e --arg id "${container_id}" --arg name "${container_name}" \
    --arg invocation "${invocation}" --arg image "${JANKURAI_BUILDER_IMAGE_ID}" \
    --arg user "${build_uid}:${build_gid}" --arg source "${source_root}" --arg scratch "${scratch}" '
    .Id == $id and .Name == ("/" + $name) and .Image == $image
    and .Config.User == $user
    and .Config.Labels["org.jeryu.builder.invocation"] == $invocation
    and (.State.Running | type == "boolean")
    and (.Mounts | all(.[]; .Type == "bind" or (.Type == "tmpfs" and .Destination == "/tmp")))
    and (.Mounts | map(select(.Type == "bind") | {Source,Destination,RW}) | sort_by(.Destination)) == ([
      {Source:$source,Destination:"/opt/jeryu/jankurai",RW:false},
      {Source:($scratch+"/vendor"),Destination:"/opt/jeryu/vendor",RW:false},
      {Source:($scratch+"/cargo-config.toml"),Destination:"/usr/local/cargo/config.toml",RW:false},
      {Source:($scratch+"/target"),Destination:"/opt/jeryu/target",RW:true},
      {Source:($scratch+"/out"),Destination:"/opt/jeryu/out",RW:true}
    ] | sort_by(.Destination))
  ' "${control}/inspect.json" >/dev/null || return 1
  container_control_valid
}

container_cleanup() {
  local docker_call_limit=5
  (( create_attempted && ! container_removed )) || return 0
  # A failed/interrupted create without its private CID is unknown closure.
  # Never infer ownership from a name, image or global daemon inventory alone.
  container_inspect || return 1
  if jq -e '.State.Running' "${control}/inspect.json" >/dev/null; then
    local_docker container stop --time 1 "${container_id}" >"${control}/stop.stdout" \
      2>"${control}/stop.stderr" || return 1
    container_inspect || return 1
  fi
  jq -e '.State.Running == false' "${control}/inspect.json" >/dev/null || return 1
  local_docker container rm "${container_id}" >"${control}/remove.stdout" \
    2>"${control}/remove.stderr" || return 1
  local_docker container ls --all --no-trunc --filter "id=${container_id}" --format '{{.ID}}' \
    >"${control}/absence.stdout" 2>"${control}/absence.stderr" || return 1
  [[ ! -s "${control}/absence.stdout" && ! -s "${control}/absence.stderr" ]] || return 1
  container_removed=1
  printf 'hermetic container custody: id=%s name=%s removed=true\n' \
    "${container_id}" "${container_name}" >&2
}

control="${scratch}/control"
mkdir -m 700 -- "${control}"
control_identity="$(stat -c '%d:%i:%u:%a' -- "${control}")"
IFS= read -r invocation </proc/sys/kernel/random/uuid
[[ "${invocation}" =~ ^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$ ]] || die "invocation identity unavailable"
container_name="jeryu-jankurai-${invocation}"
printf 'hermetic container custody: control=%s name=%s\n' "${control}" "${container_name}"
create_attempted=1

# The single-quoted script expands only inside the container.
# shellcheck disable=SC2016
docker_call_limit=30 local_docker create --cidfile "${control}/cid" --name "${container_name}" \
  --label "org.jeryu.builder.invocation=${invocation}" --pull=never --user "${build_uid}:${build_gid}" \
  --network none --read-only --cap-drop ALL --security-opt no-new-privileges \
  --pids-limit 1024 --memory 6g --cpus 2 --cpuset-cpus "${build_cpus}" \
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
    test "$(nproc)" = 2
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
  ' >"${control}/create.stdout" 2>"${control}/create.stderr"
container_inspect || die "created container ownership mismatch"
[[ "$(cat "${control}/create.stdout")" == "${container_id}" ]] || die "created container ID output mismatch"
jq -e '.State.Status == "created" and .State.Running == false' "${control}/inspect.json" >/dev/null ||
  die "container was started before ownership admission"
# The fixed attachment limit leaves a bounded owner cleanup after interruption.
docker_call_limit=900 local_docker start --attach "${container_id}"
container_inspect || die "completed container ownership mismatch"
jq -e '.State.Status == "exited" and .State.Running == false and .State.ExitCode == 0' \
  "${control}/inspect.json" >/dev/null || die "container did not exit successfully"
container_cleanup || die "container removal could not be verified"

candidate="${scratch}/out/bin/jankurai"
[[ -f "${candidate}" && ! -L "${candidate}" && -x "${candidate}" ]] ||
  die "builder did not produce one executable"
[[ "$(sha256_file "${candidate}")" == "${JANKURAI_BINARY_SHA256}" ]] ||
  die "built digest mismatch"
[[ "$("${candidate}" --version 2>/dev/null)" == "${JANKURAI_VERSION}" ]] ||
  die "built version mismatch"
[[ -z "$(git -C "${source_root}" status --porcelain --untracked-files=all)" ]] ||
  die "source checkout became dirty during build"

stage="$(mktemp "${output_parent}/.$(basename "${output}").stage.XXXXXX")"
stage_identity="$(stat -c '%d:%i:%u' -- "${stage}")"
cp --no-dereference "${candidate}" "${stage}"
chmod 755 "${stage}"
[[ "$(sha256_file "${stage}")" == "${JANKURAI_BINARY_SHA256}" ]] ||
  die "staged binary digest mismatch"
# Create the destination atomically without replacing a late file or symlink.
ln -T -- "${stage}" "${output}"
rm -- "${stage}"
stage=""
printf 'hermetic Jankurai build: %s sha256=%s context=%s\n' \
  "${JANKURAI_VERSION}" "${JANKURAI_BINARY_SHA256}" \
  "${JANKURAI_BUILD_CONTEXT_SHA256}"
