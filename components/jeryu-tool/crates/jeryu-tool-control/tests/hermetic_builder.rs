#![cfg(target_os = "linux")]

use std::{
    path::Path,
    process::{Command, Output},
};

const FIXTURE: &str = r#"
tool_root="$1"
builder_file="${tool_root}/ops/build-jankurai-hermetic.sh"
umask 077
test_root="$(mktemp -d /tmp/jeryu-builder-test.XXXXXX)"
test_identity="$(stat -c '%d:%i:%u' "${test_root}")"
cleanup() {
  local status=$? mount_point link target
  [[ -d "${test_root}" && ! -L "${test_root}" &&
    "$(realpath -e "${test_root}")" == "${test_root}" &&
    "$(stat -c '%d:%i:%u' "${test_root}")" == "${test_identity}" ]] || exit 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "${mount_point}"
    [[ "${mount_point}" != "${test_root}" &&
      "${mount_point}" != "${test_root}/"* ]] || exit 1
  done </proc/self/mountinfo
  while IFS= read -r -d '' link; do
    target="$(readlink "${link}")"
    [[ "${target}" == "${test_root}" || "${target}" == "${test_root}/"* ]] || exit 1
  done < <(find "${test_root}" -xdev -type l -print0)
  rm -rf --one-file-system --preserve-root=all -- "${test_root}"
  exit "${status}"
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP
mkdir "${test_root}/source" "${test_root}/output" "${test_root}/temporary" "${test_root}/bin"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME='Builder fixture' GIT_COMMITTER_NAME='Builder fixture'
export GIT_AUTHOR_EMAIL=fixture@example.invalid GIT_COMMITTER_EMAIL=fixture@example.invalid
printf '[workspace]\nmembers = []\n' >"${test_root}/source/Cargo.toml"
printf 'version = 4\n' >"${test_root}/source/Cargo.lock"
git -C "${test_root}/source" init -q
git -C "${test_root}/source" add .
git -C "${test_root}/source" commit -qm 'Synthetic builder refusal fixture'
export JERYU_PIN_ENV="${test_root}/fixture-pin.env"
cat "${tool_root}/generated/jankurai-pin.env" >"${JERYU_PIN_ENV}"
# Only synthetic source identity is overridden; this pin cannot qualify a build.
printf '\nJANKURAI_REV="%s"\nJANKURAI_SOURCE_TREE="%s"\nJANKURAI_SOURCE_ARCHIVE_SHA256="%s"\nJANKURAI_CARGO_LOCK_SHA256="%s"\n' \
  "$(git -C "${test_root}/source" rev-parse HEAD)" \
  "$(git -C "${test_root}/source" rev-parse 'HEAD^{tree}')" \
  "$(git -C "${test_root}/source" archive --format=tar HEAD | sha256sum | cut -d' ' -f1)" \
  "$(sha256sum "${test_root}/source/Cargo.lock" | cut -d' ' -f1)" >>"${JERYU_PIN_ENV}"
source "${JERYU_PIN_ENV}"
export FIXTURE_CARGO_VERSION="${JANKURAI_CARGO_VERSION}" FIXTURE_CARGO_LOG="${test_root}/cargo.log"
cat >"${test_root}/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${2:-}" == --version ]]; then printf '%s\n' "${FIXTURE_CARGO_VERSION}"; exit 0; fi
[[ "${2:-}" == vendor && "${CARGO_NET_OFFLINE:-}" == true &&
  "${GIT_CONFIG_GLOBAL:-}" == /dev/null && "${GIT_CONFIG_NOSYSTEM:-}" == 1 ]] || exit 92
printf '%s\n' "$*" >"${FIXTURE_CARGO_LOG}"
if [[ -n "${FIXTURE_LINK_TARGET:-}" ]]; then
  ln -s "${FIXTURE_LINK_TARGET}" "${@: -1}/fixture-link"
fi
exit 71
CARGO
chmod 700 "${test_root}/bin/cargo"
export PATH="${test_root}/bin:/usr/bin:/bin" TMPDIR="${test_root}/temporary"
source_root="${test_root}/source"
output="${test_root}/output/jankurai"
refused() {
  local reason="$1"
  shift
  if "$@" >"${test_root}/failure.log" 2>&1; then
    printf 'unexpected builder success\n' >&2
    exit 1
  fi
  if ! grep -F -- "${reason}" "${test_root}/failure.log" >/dev/null; then
    cat "${test_root}/failure.log" >&2
    exit 1
  fi
}
"#;

fn execute(script: &str, socket: Option<&Path>) -> Output {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("physical Tool root");
    Command::new("bash")
        .args(["-euo", "pipefail", "-c"])
        .arg(format!("{FIXTURE}\n{script}"))
        .arg("builder-refusal-test")
        .arg(root)
        .arg(socket.unwrap_or_else(|| Path::new("")))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run builder refusal fixture")
}

fn check(output: Output) {
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run(script: &str) {
    check(execute(script, None));
}

#[test]
fn builder_refuses_source_aliases_and_occupied_outputs() {
    run(r#"
ln -s "${test_root}" "${test_root}/alias"
ln -s "${source_root}" "${test_root}/source-link"
ln -s "${test_root}/absent" "${test_root}/output/dangling"
refused 'source root contains a path alias' bash "${builder_file}" "${test_root}/alias/source" "${output}"
refused 'caller-owned physical directory' bash "${builder_file}" "${test_root}/source-link" "${output}"
refused 'output parent contains a path alias' bash "${builder_file}" "${source_root}" "${test_root}/alias/output/jankurai"
refused 'output already exists' bash "${builder_file}" "${source_root}" "${test_root}/output/dangling"
chmod 777 "${test_root}/output"
refused 'output parent is writable by another user' bash "${builder_file}" "${source_root}" "${output}"
chmod 700 "${test_root}/output"
test ! -e "${FIXTURE_CARGO_LOG}"
"#);
}

#[test]
fn builder_bounds_jobs_and_retains_linked_scratch_on_offline_failure() {
    run(r#"
allowed="$(awk '/^Cpus_allowed_list:/ {print $2}' /proc/self/status)"
first="${allowed%%[,-]*}"
last="${allowed##*[,-]}"
test "${first}" != "${last}" # Required Linux lane needs at least two CPUs.
pair="${first},${last}"
refused 'two CPUs are required' taskset -c "${first}" bash "${builder_file}" "${source_root}" "${output}"
mode="$(stat -c %a "${source_root}/Cargo.lock")"
refused 'closed vendor materialization failed offline' taskset -c "${pair}" bash "${builder_file}" "${source_root}" "${output}"
grep -F "cpuset=${pair} cpus=2 memory=6g" "${test_root}/failure.log" >/dev/null
grep -F 'vendor --locked --offline --versioned-dirs' "${FIXTURE_CARGO_LOG}" >/dev/null
test -z "$(find "${TMPDIR}" -mindepth 1 -print -quit)"
test "$(stat -c %a "${source_root}/Cargo.lock")" = "${mode}"
test ! -e "${output}"
printf 'unchanged\n' >"${test_root}/sentinel"
export FIXTURE_LINK_TARGET="${test_root}/sentinel"
refused 'retaining changed, mounted, or linked build scratch' taskset -c "${pair}" bash "${builder_file}" "${source_root}" "${output}"
mapfile -t kept < <(find "${TMPDIR}" -mindepth 1 -maxdepth 1 -type d)
test "${#kept[@]}" = 1
test "$(readlink "${kept[0]}/vendor/fixture-link")" = "${FIXTURE_LINK_TARGET}"
test "$(cat "${FIXTURE_LINK_TARGET}")" = unchanged
"#);
}

#[test]
fn local_daemon_dispatch_clears_ambient_config_and_refuses_socket_replacement() {
    use std::{
        fs::{self, DirBuilder},
        os::unix::{
            fs::{DirBuilderExt, FileTypeExt, MetadataExt},
            net::UnixListener,
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = Path::new("/tmp").join(format!("jeryu-daemon-test-{}-{nonce}", std::process::id()));
    DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .expect("private socket directory");
    let root_identity = fs::symlink_metadata(&root).expect("socket directory identity");
    let socket = root.join("fixture.sock");
    let listener = UnixListener::bind(&socket).expect("synthetic Unix socket");
    let socket_identity = fs::symlink_metadata(&socket).expect("socket identity");
    let output = execute(
        r#"
mkdir "${test_root}/docker-config"
cat >"${test_root}/bin/fixture-engine" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
for name in ${!DOCKER_@}; do exit 93; done
[[ -z "${CREDENTIAL_SENTINEL:-}" ]] || exit 94
printf '%s\n' "$@"
ENGINE
chmod 700 "${test_root}/bin/fixture-engine"
# Extract the actual dispatcher only; the fake endpoint cannot pass production
# root-owned socket admission. No Docker executable or build is invoked.
sed -n '/^local_docker() {$/,/^}$/p' "${builder_file}" >"${test_root}/dispatch.sh"
test -s "${test_root}/dispatch.sh"
source "${test_root}/dispatch.sh"
die() { printf '%s\n' "$*" >&2; exit 1; }
docker_bin="${test_root}/bin/fixture-engine"
docker_socket="$2"
docker_socket_identity="$(stat -c '%d:%i:%u:%g:%a:%h' "${docker_socket}")"
scratch="${test_root}"
export DOCKER_HOST=ssh://fixture.invalid DOCKER_CONTEXT=fixture-remote
export DOCKER_CONFIG=/fixture-private-config DOCKER_TLS_VERIFY=1
export DOCKER_AUTH_CONFIG=fixture-auth CREDENTIAL_SENTINEL=fixture-only
local_docker image inspect --format '{{.Id}}' fixture-image >"${test_root}/dispatch.log"
printf '%s\n' --host "unix://${docker_socket}" --config "${scratch}/docker-config" \
  image inspect --format '{{.Id}}' fixture-image >"${test_root}/expected.log"
cmp "${test_root}/dispatch.log" "${test_root}/expected.log"
test -z "$(find "${test_root}/docker-config" -mindepth 1 -print -quit)"
# Run failed dispatch in a subshell: production die must terminate its process.
dispatch_failure() ( local_docker image inspect )
held_identity="${docker_socket_identity}"
docker_socket_identity=changed
refused 'local Docker socket changed after admission' dispatch_failure
docker_socket_identity="${held_identity}"
held_socket="${docker_socket}"
docker_socket="${test_root}/socket-link"
ln -s "${held_socket}" "${docker_socket}"
refused 'local Docker socket changed after admission' dispatch_failure
test "$(readlink "${docker_socket}")" = "${held_socket}"
rm "${docker_socket}"
"#,
        Some(&socket),
    );

    drop(listener);
    let root_after = fs::symlink_metadata(&root).expect("socket directory readback");
    let socket_after = fs::symlink_metadata(&socket).expect("socket readback");
    assert_eq!(
        (root_after.dev(), root_after.ino()),
        (root_identity.dev(), root_identity.ino())
    );
    assert_eq!(
        (socket_after.dev(), socket_after.ino()),
        (socket_identity.dev(), socket_identity.ino())
    );
    assert!(root_after.is_dir() && !root_after.file_type().is_symlink());
    assert!(socket_after.file_type().is_socket() && !socket_after.file_type().is_symlink());
    assert_eq!(
        root.canonicalize().expect("physical socket directory"),
        root
    );
    let mounts = fs::read_to_string("/proc/self/mountinfo").expect("mount inventory");
    assert!(
        !mounts
            .lines()
            .filter_map(|line| line.split_whitespace().nth(4))
            .any(|mount| Path::new(mount).starts_with(&root))
    );
    fs::remove_file(&socket).expect("remove owned socket fixture");
    fs::remove_dir(root).expect("remove empty socket directory");
    check(output);
}
