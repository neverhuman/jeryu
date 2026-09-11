#!/usr/bin/env bash
# Transaction and refusal tests for install-jankurai.sh. No governed host path is touched.
set -euo pipefail
umask 077

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
installer="${here}/install-jankurai.sh"
canonical_pin="${here}/../generated/jankurai-pin.env"
canonical_tag="$(sed -n 's/^JANKURAI_TAG="\([^"]*\)"$/\1/p' "${canonical_pin}")"
[[ -n "${canonical_tag}" ]] || {
  printf 'test-install-jankurai: canonical pin has no JANKURAI_TAG\n' >&2
  exit 1
}
tmp="$(mktemp -d /tmp/test-install-jankurai.XXXXXX)"
# shellcheck source=ops/test-scratch.sh
source "${here}/test-scratch.sh"
record_test_scratch "${tmp}"
first_pid=""
second_pid=""
race_pid=""
cleanup() {
  local status=$?
  set +e
  if [[ -n "${first_pid}" ]] && kill -0 "${first_pid}" 2>/dev/null; then
    kill "${first_pid}" 2>/dev/null
    wait "${first_pid}" 2>/dev/null
  fi
  if [[ -n "${second_pid}" ]] && kill -0 "${second_pid}" 2>/dev/null; then
    kill "${second_pid}" 2>/dev/null
    wait "${second_pid}" 2>/dev/null
  fi
  if [[ -n "${race_pid}" ]] && kill -0 "${race_pid}" 2>/dev/null; then
    kill "${race_pid}" 2>/dev/null
    wait "${race_pid}" 2>/dev/null
  fi
  remove_test_scratch || status=1
  exit "${status}"
}
trap cleanup EXIT
real_git="$(command -v git)"
test_token="${tmp}/forge-token"
printf 'test-fixture-token\n' > "${test_token}"
chmod 600 "${test_token}"
export JERYU_FORGE_TOKEN_FILE="${test_token}"

fail() {
  printf 'test-install-jankurai: %s\n' "$*" >&2
  if [[ -s "${tmp}/failure.log" ]]; then
    tail -n 20 "${tmp}/failure.log" >&2
  fi
  for log in "${tmp}"/*-race.log; do
    [[ -s "${log}" ]] || continue
    tail -n 20 "${log}" >&2
  done
  exit 1
}

sha() {
  sha256sum "$1" | awk '{print $1}'
}

make_mock() {
  local path="$1" version="$2"
  printf '#!/usr/bin/env bash\nprintf '\''%%s\\n'\'' %q\n' "${version}" > "${path}"
  chmod 755 "${path}"
}

make_pin() {
  local output="$1" digest="$2"
  sed "s/^JANKURAI_BINARY_SHA256=.*/JANKURAI_BINARY_SHA256=\"${digest}\"/" \
    "${canonical_pin}" > "${output}"
}

run_test_install() {
  local root="$1" pin="$2" binary="$3"
  shift 3
  env JERYU_INSTALL_TEST_MODE=1 JERYU_INSTALL_ROOT="${root}" \
    JERYU_PIN_ENV="${pin}" JERYU_INSTALL_TEST_PREBUILT_BINARY="${binary}" \
    JERYU_RUN_ID="installer-test-$$" "$@" bash "${installer}"
}

expect_failure() {
  local description="$1"
  shift
  if "$@" >"${tmp}/failure.log" 2>&1; then
    fail "${description}: command unexpectedly succeeded"
  fi
}

wait_for_file() {
  local path="$1" description="$2"
  for _ in {1..1000}; do
    [[ -e "${path}" ]] && return 0
    sleep 0.01
  done
  fail "${description}: timed out waiting for ${path}"
}

good="${tmp}/good-jankurai"
good_b="${tmp}/good-jankurai-b"
wrong="${tmp}/wrong-jankurai"
old="${tmp}/old-jankurai"
make_mock "${good}" "jankurai 1.6.11"
cp "${good}" "${good_b}"
printf '\n# distinct concurrent fixture\n' >> "${good_b}"
make_mock "${wrong}" "jankurai 1.6.10"
make_mock "${old}" "jankurai 1.6.9"
good_sha="$(sha "${good}")"
good_b_sha="$(sha "${good_b}")"
old_sha="$(sha "${old}")"
good_pin="${tmp}/good-pin.env"
good_b_pin="${tmp}/good-b-pin.env"
bad_digest_pin="${tmp}/bad-digest-pin.env"
external_pin="${tmp}/external-pin.env"
make_pin "${good_pin}" "${good_sha}"
make_pin "${good_b_pin}" "${good_b_sha}"
make_pin "${bad_digest_pin}" "$(printf '0%.0s' {1..64})"
sed 's#^JANKURAI_REPO=.*#JANKURAI_REPO="http://127.0.0.1:8787/git/jeryu/jankurai.git"#' \
  "${good_pin}" > "${external_pin}"

# Test authority is rejected before the governed target, pin, Git double, or
# prebuilt fixture can be inspected or used.
governed_target="/home/ubuntu/.jeryu/bin/jankurai"
governed_target_state() {
  if [[ -L "${governed_target}" ]]; then
    printf 'symlink:%s' "$(stat -c '%d:%i:%u:%g:%a:%h:%N' -- "${governed_target}")"
  elif [[ -f "${governed_target}" ]]; then
    printf 'regular:%s:%s' \
      "$(stat -Lc '%d:%i:%u:%g:%a:%h' -- "${governed_target}")" \
      "$(sha "${governed_target}")"
  elif [[ -e "${governed_target}" ]]; then
    printf 'other:%s' "$(stat -c '%d:%i:%u:%g:%a:%h:%F' -- "${governed_target}")"
  else
    printf 'absent'
  fi
}
governed_before="$(governed_target_state)"
governed_git="${tmp}/governed-root-git-double"
cat > "${governed_git}" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf 'invoked\n' > "${JERYU_GOVERNED_ROOT_GIT_MARKER}"
exit 97
SH
chmod 755 "${governed_git}"
expect_failure "governed root with alternate pin" env \
  JERYU_INSTALL_TEST_MODE=1 JERYU_INSTALL_ROOT="/home/ubuntu/.jeryu" \
  JERYU_PIN_ENV="${good_pin}" bash "${installer}"
expect_failure "governed root with Git double" env \
  JERYU_INSTALL_TEST_MODE=1 JERYU_INSTALL_ROOT="/home/ubuntu/.jeryu" \
  JERYU_INSTALL_TEST_GIT_BIN="${governed_git}" \
  JERYU_GOVERNED_ROOT_GIT_MARKER="${tmp}/governed-git-invoked" bash "${installer}"
expect_failure "governed root with prebuilt fixture" env \
  JERYU_INSTALL_TEST_MODE=1 JERYU_INSTALL_ROOT="/home/ubuntu/.jeryu" \
  JERYU_INSTALL_TEST_PREBUILT_BINARY="${good}" bash "${installer}"
[[ ! -e "${tmp}/governed-git-invoked" ]] ||
  fail "governed-root refusal invoked the Git double"
[[ "$(governed_target_state)" == "${governed_before}" ]] ||
  fail "governed-root refusal changed the governed target"

# Unsafe pre-existing lock identities fail before target inspection.
lock_symlink_root="${tmp}/lock-symlink"
mkdir -p "${lock_symlink_root}/bin"
cp "${old}" "${lock_symlink_root}/bin/jankurai"
ln -s "${tmp}/lock-symlink-target" "${lock_symlink_root}/.jankurai-install.lock"
expect_failure "symlink install lock" \
  run_test_install "${lock_symlink_root}" "${good_pin}" "${good}"
[[ "$(sha "${lock_symlink_root}/bin/jankurai")" == "${old_sha}" ]] ||
  fail "symlink install lock changed the target"

lock_hardlink_root="${tmp}/lock-hardlink"
mkdir -p "${lock_hardlink_root}/bin"
cp "${old}" "${lock_hardlink_root}/bin/jankurai"
lock_hardlink_source="${tmp}/lock-hardlink-source"
printf 'lock\n' > "${lock_hardlink_source}"
ln "${lock_hardlink_source}" "${lock_hardlink_root}/.jankurai-install.lock"
expect_failure "hard-linked install lock" \
  run_test_install "${lock_hardlink_root}" "${good_pin}" "${good}"
[[ "$(sha "${lock_hardlink_root}/bin/jankurai")" == "${old_sha}" ]] ||
  fail "hard-linked install lock changed the target"

lock_mode_root="${tmp}/lock-mode"
mkdir -p "${lock_mode_root}/bin"
cp "${old}" "${lock_mode_root}/bin/jankurai"
printf 'lock\n' > "${lock_mode_root}/.jankurai-install.lock"
chmod 640 "${lock_mode_root}/.jankurai-install.lock"
expect_failure "wrong-mode install lock" \
  run_test_install "${lock_mode_root}" "${good_pin}" "${good}"
[[ "$(sha "${lock_mode_root}/bin/jankurai")" == "${old_sha}" ]] ||
  fail "wrong-mode install lock changed the target"

# Receipt and rollback ancestry must be physical beneath the authenticated
# install root. Neither symlink may turn a transaction write into an external
# write, even when the external directory is writable by the test identity.
ancestor_external="${tmp}/ancestor-external"
mkdir -p "${ancestor_external}"
ancestor_sentinel="${ancestor_external}/sentinel"
printf 'ancestor sentinel\n' > "${ancestor_sentinel}"
ancestor_sentinel_sha="$(sha "${ancestor_sentinel}")"

receipt_symlink_root="${tmp}/receipt-symlink-ancestor"
mkdir -p "${receipt_symlink_root}/bin"
cp "${old}" "${receipt_symlink_root}/bin/jankurai"
ln -s "${ancestor_external}" "${receipt_symlink_root}/receipts"
expect_failure "symlinked receipt ancestor" \
  run_test_install "${receipt_symlink_root}" "${good_pin}" "${good}"
[[ "$(sha "${receipt_symlink_root}/bin/jankurai")" == "${old_sha}" ]] ||
  fail "symlinked receipt ancestor changed the target"
[[ "$(sha "${ancestor_sentinel}")" == "${ancestor_sentinel_sha}" ]] ||
  fail "symlinked receipt ancestor changed the external sentinel"
[[ "$(find "${ancestor_external}" -mindepth 1 -maxdepth 1 -type f | wc -l)" == "1" ]] ||
  fail "symlinked receipt ancestor created an external artifact"

rollback_symlink_root="${tmp}/rollback-symlink-ancestor"
mkdir -p "${rollback_symlink_root}/bin"
cp "${old}" "${rollback_symlink_root}/bin/jankurai"
ln -s "${ancestor_external}" "${rollback_symlink_root}/rollback"
expect_failure "symlinked rollback ancestor" \
  run_test_install "${rollback_symlink_root}" "${good_pin}" "${good}"
[[ "$(sha "${rollback_symlink_root}/bin/jankurai")" == "${old_sha}" ]] ||
  fail "symlinked rollback ancestor changed the target"
[[ "$(sha "${ancestor_sentinel}")" == "${ancestor_sentinel_sha}" ]] ||
  fail "symlinked rollback ancestor changed the external sentinel"
[[ "$(find "${ancestor_external}" -mindepth 1 -maxdepth 1 -type f | wc -l)" == "1" ]] ||
  fail "symlinked rollback ancestor created an external artifact"

# Existing writable leaves must be single-link physical files. A target hard
# link could otherwise make target custody ambiguous even though rename itself
# is atomic.
hardlink_target_root="${tmp}/hardlink-target"
mkdir -p "${hardlink_target_root}/bin"
hardlink_target_sentinel="${tmp}/hardlink-target-sentinel"
cp "${old}" "${hardlink_target_sentinel}"
ln "${hardlink_target_sentinel}" "${hardlink_target_root}/bin/jankurai"
hardlink_target_sha="$(sha "${hardlink_target_sentinel}")"
expect_failure "hard-linked target" \
  run_test_install "${hardlink_target_root}" "${good_pin}" "${good}"
[[ "$(sha "${hardlink_target_sentinel}")" == "${hardlink_target_sha}" ]] ||
  fail "hard-linked target changed the external sentinel"
[[ "$(stat -Lc '%h' -- "${hardlink_target_sentinel}")" == "2" ]] ||
  fail "hard-linked target custody was destructively changed"

hardlink_rollback_root="${tmp}/hardlink-rollback"
mkdir -p "${hardlink_rollback_root}/bin" "${hardlink_rollback_root}/rollback/jankurai"
cp "${old}" "${hardlink_rollback_root}/bin/jankurai"
hardlink_rollback_sentinel="${tmp}/hardlink-rollback-sentinel"
cp "${old}" "${hardlink_rollback_sentinel}"
ln "${hardlink_rollback_sentinel}" \
  "${hardlink_rollback_root}/rollback/jankurai/${old_sha}"
hardlink_rollback_sha="$(sha "${hardlink_rollback_sentinel}")"
expect_failure "hard-linked rollback leaf" \
  run_test_install "${hardlink_rollback_root}" "${good_pin}" "${good}"
[[ "$(sha "${hardlink_rollback_root}/bin/jankurai")" == "${old_sha}" ]] ||
  fail "hard-linked rollback leaf changed the target"
[[ "$(sha "${hardlink_rollback_sentinel}")" == "${hardlink_rollback_sha}" ]] ||
  fail "hard-linked rollback leaf changed the external sentinel"

# The old PID-derived stage name is no longer an authority. Pre-seed that exact
# predecessor leaf as a symlink in the process that execs the installer; the
# unpredictable exclusive stage must ignore it and preserve the sentinel.
predictable_leaf_root="${tmp}/predictable-leaf"
predictable_leaf_sentinel="${tmp}/predictable-leaf-sentinel"
printf 'predictable leaf sentinel\n' > "${predictable_leaf_sentinel}"
predictable_leaf_sha="$(sha "${predictable_leaf_sentinel}")"
# The single-quoted body intentionally expands only inside the hostile process.
# shellcheck disable=SC2016
env JERYU_INSTALL_TEST_MODE=1 JERYU_INSTALL_ROOT="${predictable_leaf_root}" \
  JERYU_PIN_ENV="${good_pin}" JERYU_INSTALL_TEST_PREBUILT_BINARY="${good}" \
  JERYU_RUN_ID="predictable-leaf" JERYU_PID_ROOT="${predictable_leaf_root}" \
  JERYU_PID_SENTINEL="${predictable_leaf_sentinel}" JERYU_PID_INSTALLER="${installer}" \
  bash -c '
    set -euo pipefail
    mkdir -p "${JERYU_PID_ROOT}/bin"
    ln -s "${JERYU_PID_SENTINEL}" "${JERYU_PID_ROOT}/bin/.jankurai.stage.$$"
    exec bash "${JERYU_PID_INSTALLER}"
  ' >/dev/null
[[ "$(sha "${predictable_leaf_root}/bin/jankurai")" == "${good_sha}" ]] ||
  fail "predecessor predictable leaf prevented the governed install"
[[ "$(sha "${predictable_leaf_sentinel}")" == "${predictable_leaf_sha}" ]] ||
  fail "predecessor predictable leaf changed the external sentinel"

# Successful transaction and receipt-bound idempotency.
root="${tmp}/success"
run_test_install "${root}" "${good_pin}" "${good}" >/dev/null
[[ "$(sha "${root}/bin/jankurai")" == "${good_sha}" ]] || fail "success digest mismatch"
receipt_count="$(find "${root}/receipts/jankurai/sha256" -type f -name '*.json' | wc -l)"
run_test_install "${root}" "${good_pin}" "${good}" >/dev/null
[[ "$(find "${root}/receipts/jankurai/sha256" -type f -name '*.json' | wc -l)" == "${receipt_count}" ]] ||
  fail "idempotent run created a new receipt"

# A receipt with any fixed build flag changed is not accepted as idempotent.
receipt="$(find "${root}/receipts/jankurai/sha256" -type f -name '*.json' -print -quit)"
receipt_lock_identity="$(stat -Lc '%d:%i:%u:%g:%a:%h' -- \
  "${root}/.jankurai-install.lock")"
jq -e --arg lock_path "${root}/.jankurai-install.lock" \
  --arg lock_identity "${receipt_lock_identity}" '
  .test_mode == true and .source.verification == "test-fixture" and
  .governance.status == "diagnostic-candidate" and
  .governance.protected_main == false and
  .governance.protection_policy == "not-applicable" and
  .installation.lock.exclusive == true and
  .installation.lock.held_through_receipt == true and
  .installation.lock.path == $lock_path and
  .installation.lock.identity == $lock_identity and
  (.governance.manifest_commit | test("^[0-9a-f]{40}$")) and
  (.governance.manifest_tree | test("^[0-9a-f]{40}$")) and
  (.governance.manifest_sha256 | test("^[0-9a-f]{64}$"))
' "${receipt}" >/dev/null || fail "test receipt claims authoritative governance"
jq '.build.git_terminal_prompt = true' "${receipt}" > "${tmp}/tampered-receipt.json"
mv "${tmp}/tampered-receipt.json" "${receipt}"
expect_failure "receipt identity mismatch" run_test_install "${root}" "${good_pin}" "${good}" \
  JERYU_INSTALL_TEST_INTERRUPT_BEFORE_RENAME=1
[[ "$(sha "${root}/bin/jankurai")" == "${good_sha}" ]] ||
  fail "receipt mismatch changed target"

# External source pins are refused before any source command or install mutation.
external_root="${tmp}/external"
expect_failure "external source" run_test_install "${external_root}" "${external_pin}" "${good}"
[[ ! -e "${external_root}/bin/jankurai" ]] || fail "external source installed a target"

# The real-source seam always disables HTTP redirects and constrains proxy bypass
# to loopback. A Git test double proves the exact environment before refusing.
fake_git="${tmp}/git-redirect-guard"
# The single-quoted lines intentionally defer expansion to the generated test double.
# shellcheck disable=SC2016
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  '[[ "${GIT_CONFIG_COUNT:-}" == "2" ]]' \
  '[[ "${GIT_CONFIG_KEY_1:-}" == "http.followRedirects" ]]' \
  '[[ "${GIT_CONFIG_VALUE_1:-}" == "false" ]]' \
  '[[ "${NO_PROXY:-}" == "127.0.0.1,localhost,::1" ]]' \
  'printf seen > "${JERYU_REDIRECT_TEST_LOG}"' \
  'exit 88' > "${fake_git}"
chmod 755 "${fake_git}"
redirect_root="${tmp}/redirect"
expect_failure "redirect guard" env \
  JERYU_INSTALL_TEST_MODE=1 JERYU_INSTALL_ROOT="${redirect_root}" \
  JERYU_PIN_ENV="${good_pin}" JERYU_INSTALL_TEST_GIT_BIN="${fake_git}" \
  JERYU_REDIRECT_TEST_LOG="${tmp}/redirect-seen" bash "${installer}"
[[ "$(cat "${tmp}/redirect-seen")" == "seen" ]] || fail "redirect guard was not applied"
[[ ! -e "${redirect_root}/bin/jankurai" ]] || fail "redirect refusal installed a target"

# Wrong digest and wrong version are rejected before replacing the target.
before="$(sha "${root}/bin/jankurai")"
expect_failure "wrong digest" run_test_install "${root}" "${bad_digest_pin}" "${good}"
[[ "$(sha "${root}/bin/jankurai")" == "${before}" ]] || fail "wrong digest changed target"
wrong_root="${tmp}/wrong-version"
expect_failure "wrong version" run_test_install "${wrong_root}" "${good_pin}" "${wrong}"
[[ ! -e "${wrong_root}/bin/jankurai" ]] || fail "wrong version installed a target"

# An empty dependency cache proves an exact local-forge-shaped source cannot
# fetch dependencies while offline. The Git double redirects only the canonical
# source URL to a disposable local repository; every source identity check and
# the real locked Cargo build still runs.
offline_root="${tmp}/offline"
offline_source="${tmp}/offline-source"
mkdir -p "${offline_source}/crates/jankurai/src"
cat > "${offline_source}/Cargo.toml" <<'TOML'
[workspace]
members = ["crates/jankurai"]
resolver = "2"
TOML
cat > "${offline_source}/crates/jankurai/Cargo.toml" <<'TOML'
[package]
name = "jankurai"
version = "1.6.11"
edition = "2024"

[dependencies]
offline-missing = "1.0.0"
TOML
cat > "${offline_source}/crates/jankurai/src/main.rs" <<'RS'
fn main() {
    println!("jankurai 1.6.11");
}
RS
cat > "${offline_source}/Cargo.lock" <<'TOML'
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 4

[[package]]
name = "jankurai"
version = "1.6.11"
dependencies = [
 "offline-missing",
]

[[package]]
name = "offline-missing"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "0000000000000000000000000000000000000000000000000000000000000000"
TOML
git init -q "${offline_source}"
git -C "${offline_source}" config user.name installer-test
git -C "${offline_source}" config user.email installer-test@localhost
git -C "${offline_source}" add Cargo.toml Cargo.lock crates
git -C "${offline_source}" commit -q -m 'offline source fixture'
offline_rev="$(git -C "${offline_source}" rev-parse HEAD)"
offline_tree="$(git -C "${offline_source}" rev-parse 'HEAD^{tree}')"
offline_archive="$(git -C "${offline_source}" archive --format=tar HEAD | sha256sum | awk '{print $1}')"
offline_lock="$(sha "${offline_source}/Cargo.lock")"
git -C "${offline_source}" tag "${canonical_tag}"
offline_pin="${tmp}/offline-pin.env"
sed \
  -e "s/^JANKURAI_REV=.*/JANKURAI_REV=\"${offline_rev}\"/" \
  -e "s/^JANKURAI_SOURCE_TREE=.*/JANKURAI_SOURCE_TREE=\"${offline_tree}\"/" \
  -e "s/^JANKURAI_SOURCE_ARCHIVE_SHA256=.*/JANKURAI_SOURCE_ARCHIVE_SHA256=\"${offline_archive}\"/" \
  -e "s/^JANKURAI_CARGO_LOCK_SHA256=.*/JANKURAI_CARGO_LOCK_SHA256=\"${offline_lock}\"/" \
  "${good_pin}" > "${offline_pin}"
offline_git="${tmp}/git-offline-source"
cat > "${offline_git}" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "ls-remote" ]]; then
  args=("$@")
  for index in "${!args[@]}"; do
    if [[ "${args[index]}" == "${JERYU_OFFLINE_TEST_CANONICAL}" ]]; then
      args[index]="${JERYU_OFFLINE_TEST_SOURCE}"
    fi
  done
  exec "${JERYU_OFFLINE_TEST_REAL_GIT}" "${args[@]}"
fi
if [[ "$#" -eq 6 && "$1" == "-C" && "$3" == "remote" &&
      "$4" == "add" && "$5" == "origin" &&
      "$6" == "${JERYU_OFFLINE_TEST_CANONICAL}" ]]; then
  exec "${JERYU_OFFLINE_TEST_REAL_GIT}" -C "$2" remote add origin \
    "${JERYU_OFFLINE_TEST_SOURCE}"
fi
if [[ "$#" -eq 5 && "$1" == "-C" && "$3" == "remote" &&
      "$4" == "get-url" && "$5" == "origin" ]]; then
  printf '%s\n' "${JERYU_OFFLINE_TEST_CANONICAL}"
  exit 0
fi
exec "${JERYU_OFFLINE_TEST_REAL_GIT}" "$@"
SH
chmod 755 "${offline_git}"
mkdir -p "${tmp}/empty-cargo/registry"
if env JERYU_INSTALL_TEST_MODE=1 JERYU_INSTALL_ROOT="${offline_root}" \
  JERYU_PIN_ENV="${offline_pin}" JERYU_INSTALL_TEST_GIT_BIN="${offline_git}" \
  JERYU_OFFLINE_TEST_REAL_GIT="${real_git}" \
  JERYU_OFFLINE_TEST_SOURCE="${offline_source}" \
  JERYU_OFFLINE_TEST_CANONICAL="https://github.com/neverhuman/jankurai.git" \
  JERYU_CARGO_CACHE_SEED="${tmp}/empty-cargo" JERYU_RUN_ID="offline-test-$$" \
  bash "${installer}" >"${tmp}/offline.log" 2>&1; then
  fail "offline fetch test unexpectedly succeeded"
fi
[[ ! -e "${offline_root}/bin/jankurai" ]] || fail "offline failure installed a target"
grep -Eqi 'offline|no matching package|failed to download' "${tmp}/offline.log" || {
  cp "${tmp}/offline.log" "${tmp}/failure.log"
  fail "offline refusal did not report an offline dependency failure"
}

# Interruption before rename leaves the previous binary byte-identical.
interrupt_root="${tmp}/interrupt"
mkdir -p "${interrupt_root}/bin"
cp "${old}" "${interrupt_root}/bin/jankurai"
expect_failure "interrupted install" run_test_install "${interrupt_root}" "${good_pin}" "${good}" \
  JERYU_INSTALL_TEST_INTERRUPT_BEFORE_RENAME=1
[[ "$(sha "${interrupt_root}/bin/jankurai")" == "${old_sha}" ]] ||
  fail "interrupted install changed target"

# A post-rename failure restores the previous content-addressed rollback artifact.
rollback_root="${tmp}/rollback"
mkdir -p "${rollback_root}/bin"
cp "${old}" "${rollback_root}/bin/jankurai"
expect_failure "post-rename rollback" run_test_install "${rollback_root}" "${good_pin}" "${good}" \
  JERYU_INSTALL_TEST_FAIL_AFTER_RENAME=1
[[ "$(sha "${rollback_root}/bin/jankurai")" == "${old_sha}" ]] ||
  fail "rollback did not restore previous binary"
[[ "$(sha "${rollback_root}/rollback/jankurai/${old_sha}")" == "${old_sha}" ]] ||
  fail "rollback artifact is missing or corrupt"

# Replacement after directory descriptors are retained must fail before any
# candidate or receipt byte reaches the replacement symlink target.
directory_race_root="${tmp}/directory-replacement-race"
directory_race_external="${tmp}/directory-replacement-external"
mkdir -p "${directory_race_external}"
directory_race_sentinel="${directory_race_external}/sentinel"
printf 'directory race sentinel\n' > "${directory_race_sentinel}"
directory_race_sentinel_sha="$(sha "${directory_race_sentinel}")"
directory_race_ready="${tmp}/directory-race-ready"
directory_race_release="${tmp}/directory-race-release"
run_test_install "${directory_race_root}" "${good_pin}" "${good}" \
  JERYU_INSTALL_TEST_PAUSE_AFTER_CUSTODY_READY_FILE="${directory_race_ready}" \
  JERYU_INSTALL_TEST_PAUSE_AFTER_CUSTODY_RELEASE_FILE="${directory_race_release}" \
  >"${tmp}/directory-race.log" 2>&1 &
race_pid=$!
wait_for_file "${directory_race_ready}" "directory replacement installer"
mv "${directory_race_root}/receipts" "${directory_race_root}/receipts-retained"
ln -s "${directory_race_external}" "${directory_race_root}/receipts"
printf 'release\n' > "${directory_race_release}"
if wait "${race_pid}"; then
  fail "directory replacement race unexpectedly succeeded"
fi
race_pid=""
[[ ! -e "${directory_race_root}/bin/jankurai" ]] ||
  fail "directory replacement race installed a target"
[[ "$(sha "${directory_race_sentinel}")" == "${directory_race_sentinel_sha}" ]] ||
  fail "directory replacement race changed the external sentinel"
[[ "$(find "${directory_race_external}" -mindepth 1 -maxdepth 1 -type f | wc -l)" == "1" ]] ||
  fail "directory replacement race created an external artifact"

# Replacement of the unpredictable stage name after exclusive creation must
# also fail. The installer writes only through the retained descriptor and
# verifies its named link before rename, so the attacker-selected sentinel is
# never opened for writing.
leaf_race_root="${tmp}/leaf-replacement-race"
leaf_race_sentinel="${tmp}/leaf-replacement-sentinel"
printf 'leaf race sentinel\n' > "${leaf_race_sentinel}"
leaf_race_sentinel_sha="$(sha "${leaf_race_sentinel}")"
leaf_race_ready="${tmp}/leaf-race-ready"
leaf_race_release="${tmp}/leaf-race-release"
run_test_install "${leaf_race_root}" "${good_pin}" "${good}" \
  JERYU_INSTALL_TEST_PAUSE_BEFORE_STAGE_RENAME_READY_FILE="${leaf_race_ready}" \
  JERYU_INSTALL_TEST_PAUSE_BEFORE_STAGE_RENAME_RELEASE_FILE="${leaf_race_release}" \
  >"${tmp}/leaf-race.log" 2>&1 &
race_pid=$!
wait_for_file "${leaf_race_ready}" "leaf replacement installer"
mapfile -t leaf_race_stages < <(
  find "${leaf_race_root}/bin" -maxdepth 1 -type f -name '.jankurai.stage.*' -print
)
[[ "${#leaf_race_stages[@]}" == "1" ]] ||
  fail "leaf replacement race did not expose exactly one retained stage"
leaf_race_name="$(basename "${leaf_race_stages[0]}")"
[[ "${leaf_race_name}" =~ ^\.jankurai\.stage\.[0-9a-f-]{36}$ ]] ||
  fail "transaction stage name is not an unpredictable kernel identity"
rm -f -- "${leaf_race_stages[0]}"
ln -s "${leaf_race_sentinel}" "${leaf_race_stages[0]}"
printf 'release\n' > "${leaf_race_release}"
if wait "${race_pid}"; then
  fail "leaf replacement race unexpectedly succeeded"
fi
race_pid=""
[[ ! -e "${leaf_race_root}/bin/jankurai" ]] ||
  fail "leaf replacement race installed a target"
[[ "$(sha "${leaf_race_sentinel}")" == "${leaf_race_sentinel_sha}" ]] ||
  fail "leaf replacement race changed the external sentinel"

# The lock serializes the complete target/rollback/receipt transaction. The
# second install reaches the lock but cannot acquire it while the first pauses
# after replacement; after serialization, its simulated failure restores the
# first successful install rather than the original target.
concurrent_root="${tmp}/concurrent"
mkdir -p "${concurrent_root}/bin"
cp "${old}" "${concurrent_root}/bin/jankurai"
first_ready="${tmp}/concurrent-first-ready"
first_release="${tmp}/concurrent-first-release"
first_acquired="${tmp}/concurrent-first-acquired"
second_waiting="${tmp}/concurrent-second-waiting"
second_acquired="${tmp}/concurrent-second-acquired"
run_test_install "${concurrent_root}" "${good_pin}" "${good}" \
  JERYU_RUN_ID="concurrent-first" \
  JERYU_INSTALL_TEST_LOCK_ACQUIRED_FILE="${first_acquired}" \
  JERYU_INSTALL_TEST_PAUSE_AFTER_RENAME_READY_FILE="${first_ready}" \
  JERYU_INSTALL_TEST_PAUSE_AFTER_RENAME_RELEASE_FILE="${first_release}" \
  >"${tmp}/concurrent-first.log" 2>&1 &
first_pid=$!
wait_for_file "${first_ready}" "first concurrent installer"
[[ -e "${first_acquired}" ]] || fail "first concurrent installer never acquired the lock"
[[ "$(sha "${concurrent_root}/bin/jankurai")" == "${good_sha}" ]] ||
  fail "first concurrent installer did not pause after replacement"

run_test_install "${concurrent_root}" "${good_b_pin}" "${good_b}" \
  JERYU_RUN_ID="concurrent-second" \
  JERYU_INSTALL_TEST_LOCK_WAITING_FILE="${second_waiting}" \
  JERYU_INSTALL_TEST_LOCK_ACQUIRED_FILE="${second_acquired}" \
  JERYU_INSTALL_TEST_FAIL_AFTER_RENAME=1 \
  >"${tmp}/concurrent-second.log" 2>&1 &
second_pid=$!
wait_for_file "${second_waiting}" "second concurrent installer"
[[ ! -e "${second_acquired}" ]] ||
  fail "second concurrent installer entered while the first held the lock"
kill -0 "${second_pid}" 2>/dev/null ||
  fail "second concurrent installer exited instead of waiting for the lock"

printf 'release\n' > "${first_release}"
if ! wait "${first_pid}"; then
  fail "first concurrent installer failed: $(tail -n 1 "${tmp}/concurrent-first.log")"
fi
first_pid=""
if wait "${second_pid}"; then
  fail "second concurrent installer unexpectedly succeeded"
fi
second_pid=""
[[ -e "${second_acquired}" ]] ||
  fail "second concurrent installer never acquired the released lock"
[[ "$(sha "${concurrent_root}/bin/jankurai")" == "${good_sha}" ]] ||
  fail "serialized rollback did not restore the first successful target"
[[ "$(sha "${concurrent_root}/rollback/jankurai/${old_sha}")" == "${old_sha}" ]] ||
  fail "concurrent rollback chain lost the original target"
[[ "$(sha "${concurrent_root}/rollback/jankurai/${good_sha}")" == "${good_sha}" ]] ||
  fail "concurrent rollback chain lost the first successful target"
[[ ! -e "${concurrent_root}/rollback/jankurai/${good_b_sha}" ]] ||
  fail "failed concurrent candidate became rollback authority"
[[ "$(find "${concurrent_root}/bin" -maxdepth 1 -name '.jankurai.*' | wc -l)" == "0" ]] ||
  fail "concurrent install left a staged target"
concurrent_receipt_count="$(
  find "${concurrent_root}/receipts/jankurai/sha256" -type f -name '*.json' | wc -l
)"
[[ "${concurrent_receipt_count}" == "1" ]] ||
  fail "concurrent install did not produce exactly one successful receipt"
concurrent_receipt="$(
  find "${concurrent_root}/receipts/jankurai/sha256" -type f -name '*.json' -print -quit
)"
[[ "$(sha "${concurrent_receipt}")" == "$(basename "${concurrent_receipt}" .json)" ]] ||
  fail "concurrent receipt is not content-addressed"
concurrent_lock_identity="$(stat -Lc '%d:%i:%u:%g:%a:%h' -- \
  "${concurrent_root}/.jankurai-install.lock")"
jq -e --arg current "${good_sha}" --arg previous "${old_sha}" \
  --arg lock_path "${concurrent_root}/.jankurai-install.lock" \
  --arg lock_identity "${concurrent_lock_identity}" \
  '.conclusion == "success" and .run_id == "concurrent-first" and
   .binary.sha256 == $current and
   .installation.previous_binary_sha256 == $previous and
   .installation.lock.exclusive == true and
   .installation.lock.held_through_receipt == true and
   .installation.lock.path == $lock_path and
   .installation.lock.identity == $lock_identity' \
  "${concurrent_receipt}" >/dev/null ||
  fail "concurrent receipt is not bound to the successful transaction"
[[ -f "${concurrent_root}/.jankurai-install.lock" &&
   ! -L "${concurrent_root}/.jankurai-install.lock" ]] ||
  fail "install lock is not a physical file"
[[ "$(stat -Lc '%a:%h' -- "${concurrent_root}/.jankurai-install.lock")" == "600:1" ]] ||
  fail "install lock custody metadata is invalid"

# A pre-existing content-addressed rollback name with corrupt bytes is rejected
# before the governed target can be replaced.
corrupt_root="${tmp}/corrupt-rollback"
mkdir -p "${corrupt_root}/bin" "${corrupt_root}/rollback/jankurai"
cp "${old}" "${corrupt_root}/bin/jankurai"
printf 'corrupt rollback bytes\n' > "${corrupt_root}/rollback/jankurai/${old_sha}"
expect_failure "corrupt rollback artifact" \
  run_test_install "${corrupt_root}" "${good_pin}" "${good}"
[[ "$(sha "${corrupt_root}/bin/jankurai")" == "${old_sha}" ]] ||
  fail "corrupt rollback artifact changed target"

printf 'install-jankurai tests passed: governed-root lock-custody physical-ancestors exclusive-leaves hard-links replacement-races external-sentinels success idempotency receipt-governance external-source redirect-guard wrong-digest wrong-version offline interruption rollback concurrent-lock corrupt-rollback\n'
