#!/usr/bin/env bash
# Temporarily bind one independently qualified PR candidate to the root broker
# for exactly one Jeryu Tool required-check attempt, then restore protected main.
# shellcheck disable=SC2317 # trap callbacks are reached indirectly by Bash.
set -Eeuo pipefail
umask 077

readonly production_repo_root="/home/ubuntu/jain-split/jeryu-split/jeryu-tool"
readonly production_install_dir="/usr/local/libexec/jain"
readonly production_state_root="/var/lib/jain-host-ci/jeryu-tool-root-seal-bootstrap"
readonly production_entrypoint="${production_install_dir}/bootstrap-jankurai-root-seal"
readonly production_authority_config="${production_install_dir}/jeryu-tool-root-seal.config.json"
readonly production_pin_env="${production_install_dir}/jeryu-tool-root-seal-pin.env"
readonly production_splitops_config="${production_install_dir}/native-build-tools-installer.config.json"
readonly production_splitctl="${production_install_dir}/splitctl"
readonly production_token_file="${production_install_dir}/jeryu-merge-token"
readonly production_remote="http://127.0.0.1:8787/git/jeryu/jeryu-tool.git"
readonly production_splitops_remote="http://127.0.0.1:8787/git/veox/jain-split-ops.git"
readonly production_predecessor_sha256="96d99e6e7d8dc9cf23df1081edd1f975231456592f81d9405385219a2c7298aa"
readonly production_candidate_sha256="9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c"
readonly maximum_lifetime_seconds=900

fail() {
  printf 'bootstrap-jankurai-root-seal: %s\n' "$*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required tool: $1"
}

sha256_file() {
  sha256sum -- "$1" | awk '{print $1}'
}

file_identity() {
  stat -Lc '%d:%i:%u:%g:%a:%h' -- "$1"
}

require_physical_file() {
  local path="$1" description="$2"
  [[ "$path" == /* && -f "$path" && ! -L "$path" \
    && "$(stat -Lc '%h' -- "$path" 2>/dev/null)" == 1 \
    && "$(realpath -e -- "$path" 2>/dev/null)" == "$path" ]] \
    || fail "$description must be an absolute, physical, single-link regular file"
}

require_physical_dir() {
  local path="$1" description="$2"
  [[ "$path" == /* && -d "$path" && ! -L "$path" \
    && "$(realpath -e -- "$path" 2>/dev/null)" == "$path" ]] \
    || fail "$description must be an absolute physical directory"
}

require_immutable_ancestry() {
  local path="$1" boundary="$2" description="$3" cursor
  cursor="$(realpath -e -- "$path")" ||
    fail "$description ancestry cannot be resolved"
  boundary="$(realpath -e -- "$boundary")" ||
    fail "$description boundary cannot be resolved"
  while :; do
    [[ ! -L "$cursor" \
      && "$(stat -Lc '%u:%g' -- "$cursor")" == "$authority_uid:$authority_gid" \
      && $((8#$(stat -Lc '%a' -- "$cursor") & 8#022)) == 0 ]] ||
      fail "$description ancestry is not authority-owned and immutable"
    [[ "$cursor" == "$boundary" ]] && break
    [[ "$cursor" != / ]] ||
      fail "$description ancestry escaped its authority boundary"
    cursor="$(dirname -- "$cursor")"
  done
}

assert_held_file() {
  local path="$1" descriptor="$2" mode="$3" description="$4"
  [[ "$path" == /* && -f "$path" && ! -L "$path" \
    && "$(realpath -e -- "$path")" == "$path" \
    && "$(stat -Lc '%u:%g:%a:%h' -- "$path")" \
      == "$authority_uid:$authority_gid:$mode:1" \
    && "$(stat -Lc '%d:%i' -- "$path")" \
      == "$(stat -Lc '%d:%i' -- "$descriptor")" \
    && "$(readlink -f -- "$descriptor")" == "$path" ]] ||
    fail "unsafe held $description"
}

tagged_blob_sha256() {
  local root="$1" commit="$2" relative="$3"
  [[ "$relative" =~ ^[A-Za-z0-9._/-]+$ && "$relative" != /* \
    && "$relative" != *..* && "$relative" != *//* ]] ||
    fail 'unsafe Git blob path'
  git -c core.hooksPath=/dev/null -c core.fsmonitor=false \
    -c diff.external= -c filter.lfs.process= -c filter.lfs.smudge= \
    -c filter.lfs.clean= -c filter.lfs.required=false \
    -C "$root" cat-file blob "$commit:$relative" |
    sha256sum | awk '{print $1}'
}

durable_sync() {
  [[ "$test_mode" == 1 ]] || sync -f "$1"
}

test_mode="${JERYU_BOOTSTRAP_TEST_MODE:-0}"
[[ "$test_mode" == 0 || "$test_mode" == 1 ]] ||
  fail 'JERYU_BOOTSTRAP_TEST_MODE must be 0 or 1'
if [[ "$test_mode" == 0 ]]; then
  [[ "$(id -u)" == 0 ]] || fail 'production bootstrap is root-only'
  for variable in \
    JERYU_BOOTSTRAP_REPO_ROOT JERYU_BOOTSTRAP_INSTALL_DIR \
    JERYU_BOOTSTRAP_STATE_ROOT JERYU_BOOTSTRAP_ENTRYPOINT \
    JERYU_BOOTSTRAP_AUTHORITY_CONFIG JERYU_BOOTSTRAP_SPLITOPS_CONFIG \
    JERYU_BOOTSTRAP_SPLITCTL JERYU_BOOTSTRAP_TOKEN_FILE \
    JERYU_BOOTSTRAP_AUTHORITY_ROOT JERYU_BOOTSTRAP_REMOTE \
    JERYU_BOOTSTRAP_PIN_ENV \
    JERYU_BOOTSTRAP_EXPECTED_PREDECESSOR_SHA256 JERYU_BOOTSTRAP_NOW \
    GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_CONFIG_GLOBAL GIT_CONFIG_SYSTEM \
    JERYU_BOOTSTRAP_TEST_PAUSE_READY_FILE \
    JERYU_BOOTSTRAP_TEST_PAUSE_RELEASE_FILE \
    JERYU_BOOTSTRAP_TEST_ALLOW_HEAD_REUSE; do
    [[ ! -v "$variable" ]] || fail "production override is forbidden: $variable"
  done
fi

[[ "$#" -eq 1 ]] || {
  printf 'usage: %s ABSOLUTE_BOOTSTRAP_REQUEST.json\n' "$0" >&2
  exit 2
}

for tool in awk bash chmod chown cmp cp cut date env find flock git jq mkdir \
  mktemp mv realpath rm setpriv setsid sha256sum stat sync; do
  need "$tool"
done

authority_uid="$(id -u)"
authority_gid="$(id -g)"
authority_root="${JERYU_BOOTSTRAP_AUTHORITY_ROOT:-/}"
entrypoint="${JERYU_BOOTSTRAP_ENTRYPOINT:-${production_entrypoint}}"
authority_config="${JERYU_BOOTSTRAP_AUTHORITY_CONFIG:-${production_authority_config}}"
splitops_config="${JERYU_BOOTSTRAP_SPLITOPS_CONFIG:-${production_splitops_config}}"
splitctl="${JERYU_BOOTSTRAP_SPLITCTL:-${production_splitctl}}"
token_file="${JERYU_BOOTSTRAP_TOKEN_FILE:-${production_token_file}}"
pin_env="${JERYU_BOOTSTRAP_PIN_ENV:-${production_pin_env}}"
repo_root="${JERYU_BOOTSTRAP_REPO_ROOT:-${production_repo_root}}"
install_dir="${JERYU_BOOTSTRAP_INSTALL_DIR:-${production_install_dir}}"
state_root="${JERYU_BOOTSTRAP_STATE_ROOT:-${production_state_root}}"
runner_custody_root="${state_root}-runner-custody"
manifest_remote="${JERYU_BOOTSTRAP_REMOTE:-${production_remote}}"
now_epoch="${JERYU_BOOTSTRAP_NOW:-$(date +%s)}"

[[ "$now_epoch" =~ ^[0-9]+$ ]] || fail 'current epoch is malformed'
if [[ "$test_mode" == 0 ]]; then
  [[ "$authority_uid:$authority_gid" == 0:0 \
    && "$authority_root" == / \
    && "$entrypoint" == "$production_entrypoint" \
    && "$authority_config" == "$production_authority_config" \
    && "$splitops_config" == "$production_splitops_config" \
    && "$splitctl" == "$production_splitctl" \
    && "$token_file" == "$production_token_file" \
    && "$pin_env" == "$production_pin_env" \
    && "$repo_root" == "$production_repo_root" \
    && "$install_dir" == "$production_install_dir" \
    && "$state_root" == "$production_state_root" \
    && "$manifest_remote" == "$production_remote" ]] \
    || fail 'production bootstrap authority differs from compiled constants'
fi

actual_entrypoint="$(realpath -e -- "${BASH_SOURCE[0]}")" ||
  fail 'cannot resolve bootstrap entrypoint'
[[ "$actual_entrypoint" == "$entrypoint" ]] ||
  fail 'production bootstrap must execute the installed entrypoint'
require_physical_dir "$repo_root" 'Jeryu Tool repository'
require_physical_dir "$install_dir" 'host broker install directory'
require_physical_file "$entrypoint" 'root-seal bootstrap entrypoint'
require_physical_file "$authority_config" 'root-seal bootstrap authority config'
require_physical_file "$splitops_config" 'installed SplitOps authority config'
require_physical_file "$splitctl" 'installed SplitOps broker'
require_physical_file "$token_file" 'installed forge token'
require_physical_file "$pin_env" 'Jankurai pin authority'
require_immutable_ancestry "$install_dir" "$authority_root" \
  'root-seal installed authority'

exec {entrypoint_fd}<"$entrypoint"
exec {authority_fd}<"$authority_config"
exec {splitops_config_fd}<"$splitops_config"
exec {splitctl_fd}<"$splitctl"
exec {token_fd}<"$token_file"
exec {pin_fd}<"$pin_env"
entrypoint_descriptor="/proc/${BASHPID}/fd/${entrypoint_fd}"
authority_descriptor="/proc/${BASHPID}/fd/${authority_fd}"
splitops_config_descriptor="/proc/${BASHPID}/fd/${splitops_config_fd}"
splitctl_descriptor="/proc/${BASHPID}/fd/${splitctl_fd}"
token_descriptor="/proc/${BASHPID}/fd/${token_fd}"
pin_descriptor="/proc/${BASHPID}/fd/${pin_fd}"
assert_held_file "$entrypoint" "$entrypoint_descriptor" 500 \
  'root-seal bootstrap entrypoint'
assert_held_file "$authority_config" "$authority_descriptor" 600 \
  'root-seal bootstrap authority config'
assert_held_file "$splitops_config" "$splitops_config_descriptor" 600 \
  'installed SplitOps authority config'
assert_held_file "$splitctl" "$splitctl_descriptor" 500 \
  'installed SplitOps broker'
assert_held_file "$token_file" "$token_descriptor" 600 \
  'installed forge token'
assert_held_file "$pin_env" "$pin_descriptor" 400 \
  'installed Jankurai pin'

jq -e '
  select(type == "object")
  | select(keys == [
      "bootstrap_sha256", "control_commit", "control_ref", "control_remote",
      "control_tree", "entrypoint_path", "expected_predecessor_sha256",
      "pin_path", "pin_sha256", "schema_version", "splitctl_path",
      "splitops_config_path", "state_root", "token_file"
    ])
  | select(.schema_version == "jeryu.jankurai-root-seal-authority/v1")
  | select(.bootstrap_sha256 | test("^[0-9a-f]{64}$"))
  | select(.control_commit | test("^[0-9a-f]{40}$"))
  | select(.control_ref
      == "refs/heads/codex/jeryu-tool-jankurai-split3-production-digest-r17-20260802")
  | select(.control_remote
      == "http://127.0.0.1:8787/git/jeryu/jeryu-tool.git")
  | select(.control_tree | test("^[0-9a-f]{40}$"))
  | select(.entrypoint_path | type == "string" and startswith("/"))
  | select(.expected_predecessor_sha256 | test("^[0-9a-f]{64}$"))
  | select(.pin_path | type == "string" and startswith("/"))
  | select(.pin_sha256 | test("^[0-9a-f]{64}$"))
  | select(.splitctl_path | type == "string" and startswith("/"))
  | select(.splitops_config_path | type == "string" and startswith("/"))
  | select(.state_root | type == "string" and startswith("/"))
  | select(.token_file | type == "string" and startswith("/"))
' "$authority_descriptor" >/dev/null ||
  fail 'installed root-seal bootstrap authority config is invalid'

control_ref="$(jq -er '.control_ref' "$authority_descriptor")"
authority_head="$(jq -er '.control_commit' "$authority_descriptor")"
authority_tree="$(jq -er '.control_tree' "$authority_descriptor")"
expected_predecessor="$(jq -er '.expected_predecessor_sha256' \
  "$authority_descriptor")"
[[ "$(jq -er '.entrypoint_path' "$authority_descriptor")" == "$entrypoint" \
  && "$(jq -er '.pin_path' "$authority_descriptor")" == "$pin_env" \
  && "$(jq -er '.splitops_config_path' "$authority_descriptor")" \
    == "$splitops_config" \
  && "$(jq -er '.splitctl_path' "$authority_descriptor")" == "$splitctl" \
  && "$(jq -er '.state_root' "$authority_descriptor")" == "$state_root" \
  && "$(jq -er '.token_file' "$authority_descriptor")" == "$token_file" \
  && "$(jq -er '.control_remote' "$authority_descriptor")" == "$manifest_remote" \
  && "$(sha256_file "$entrypoint_descriptor")" \
    == "$(jq -er '.bootstrap_sha256' "$authority_descriptor")" \
  && "$(sha256_file "$pin_descriptor")" \
    == "$(jq -er '.pin_sha256' "$authority_descriptor")" ]] ||
  fail 'installed root-seal bootstrap authority binding is inconsistent'
[[ "$expected_predecessor" =~ ^[0-9a-f]{64}$ ]] ||
  fail 'protected predecessor digest is malformed'
if [[ "$test_mode" == 0 ]]; then
  [[ "$expected_predecessor" == "$production_predecessor_sha256" ]] ||
    fail 'production predecessor differs from the reviewed protected binary'
fi

jq -e '
  select(type == "object")
  | select(keys == [
      "bootstrap_expires_at", "control_commit", "control_ref",
      "control_remote", "control_tag_ref", "install_dir", "schema_version",
      "splitctl_sha256", "token_file"
    ])
  | select(.schema_version == "jain.native-build-tools-installer-config/v2")
  | select(.bootstrap_expires_at == "")
  | select(.control_ref == "refs/heads/main")
  | select(.control_commit | test("^[0-9a-f]{40}$"))
  | select(.control_tag_ref
      | test("^refs/tags/jain-split-ops-v[0-9]+\\.[0-9]+\\.[0-9]+-split\\.[0-9]+$"))
  | select(.splitctl_sha256 | test("^[0-9a-f]{64}$"))
' "$splitops_config_descriptor" >/dev/null ||
  fail 'installed SplitOps authority config is invalid'
ops_remote="$(jq -er '.control_remote' "$splitops_config_descriptor")"
ops_ref="$(jq -er '.control_ref' "$splitops_config_descriptor")"
ops_commit="$(jq -er '.control_commit' "$splitops_config_descriptor")"
ops_tag_ref="$(jq -er '.control_tag_ref' "$splitops_config_descriptor")"
[[ "$(jq -er '.install_dir' "$splitops_config_descriptor")" == "$install_dir" \
  && "$(jq -er '.token_file' "$splitops_config_descriptor")" == "$token_file" \
  && "$(sha256_file "$splitctl_descriptor")" \
    == "$(jq -er '.splitctl_sha256' "$splitops_config_descriptor")" ]] ||
  fail 'installed SplitOps broker/config binding is inconsistent'
[[ "$ops_remote" == "$production_splitops_remote" ]] ||
  fail 'installed SplitOps authority does not name the fixed forge'
entrypoint_sha256="$(sha256_file "$entrypoint_descriptor")"
authority_config_sha256="$(sha256_file "$authority_descriptor")"
splitops_config_sha256="$(sha256_file "$splitops_config_descriptor")"
splitctl_sha256="$(sha256_file "$splitctl_descriptor")"
token_sha256="$(sha256_file "$token_descriptor")"
pin_sha256="$(sha256_file "$pin_descriptor")"

assert_installed_authority_held() {
  assert_held_file "$entrypoint" "$entrypoint_descriptor" 500 \
    'root-seal bootstrap entrypoint'
  assert_held_file "$authority_config" "$authority_descriptor" 600 \
    'root-seal bootstrap authority config'
  assert_held_file "$splitops_config" "$splitops_config_descriptor" 600 \
    'installed SplitOps authority config'
  assert_held_file "$splitctl" "$splitctl_descriptor" 500 \
    'installed SplitOps broker'
  assert_held_file "$token_file" "$token_descriptor" 600 \
    'installed forge token'
  assert_held_file "$pin_env" "$pin_descriptor" 400 \
    'installed Jankurai pin'
  [[ "$(sha256_file "$entrypoint_descriptor")" == "$entrypoint_sha256" \
    && "$(sha256_file "$entrypoint")" == "$entrypoint_sha256" \
    && "$(sha256_file "$authority_descriptor")" == "$authority_config_sha256" \
    && "$(sha256_file "$authority_config")" == "$authority_config_sha256" \
    && "$(sha256_file "$splitops_config_descriptor")" == "$splitops_config_sha256" \
    && "$(sha256_file "$splitops_config")" == "$splitops_config_sha256" \
    && "$(sha256_file "$splitctl_descriptor")" == "$splitctl_sha256" \
    && "$(sha256_file "$splitctl")" == "$splitctl_sha256" \
    && "$(sha256_file "$token_descriptor")" == "$token_sha256" \
    && "$(sha256_file "$token_file")" == "$token_sha256" \
    && "$(sha256_file "$pin_descriptor")" == "$pin_sha256" \
    && "$(sha256_file "$pin_env")" == "$pin_sha256" ]] ||
    fail 'installed root-seal authority identity or content drift detected'
}

mkdir -p -- "$state_root"
chmod 0700 -- "$state_root"
require_physical_dir "$state_root" 'bootstrap state root'
[[ "$(stat -Lc '%u:%g:%a' -- "$state_root")" \
  == "$authority_uid:$authority_gid:700" ]] ||
  fail 'bootstrap state root custody is unsafe'
authority_scratch="$(mktemp -d "${state_root}.authority.XXXXXX")"
chmod 0700 "$authority_scratch"
cleanup_authority_scratch() {
  if [[ -n "${authority_scratch:-}" \
    && "$authority_scratch" == "${state_root}.authority."?????? \
    && -d "$authority_scratch" && ! -L "$authority_scratch" \
    && "$(stat -Lc '%u:%g' -- "$authority_scratch")" \
      == "$authority_uid:$authority_gid" ]]; then
    chmod -R u+w "$authority_scratch" 2>/dev/null || true
    rm -rf --one-file-system -- "$authority_scratch"
  fi
}
trap cleanup_authority_scratch EXIT
source_root="$authority_scratch/jeryu-tool"
source_materialization="$authority_scratch/jeryu-tool.json"
"$splitctl_descriptor" jeryu-local git-materialize \
  --repo jeryu/jeryu-tool \
  --remote "$manifest_remote" \
  --ref "$control_ref" \
  --expected-head "$authority_head" \
  --destination "$source_root" \
  --token-file "$token_file" >"$source_materialization" ||
  fail 'cannot authenticate the installed Jeryu Tool entrypoint source'
jq -e \
  --arg remote "$manifest_remote" \
  --arg reference "$control_ref" \
  --arg commit "$authority_head" \
  --arg destination "$source_root" '
    select(.schema_version == "jain.jeryu-git-materialization/v1")
    | select(.repository == "jeryu/jeryu-tool")
    | select(.remote == $remote and .reference == $reference)
    | select(.commit == $commit and .destination == $destination)
    | select(.origin_retained == false and .lfs_hydrated == false)
    | select(.status == "pass")
  ' "$source_materialization" >/dev/null ||
  fail 'authenticated Jeryu Tool materialization report is invalid'
[[ "$(git -C "$source_root" rev-parse --verify 'HEAD^{commit}')" \
    == "$authority_head" \
  && "$(git -C "$source_root" rev-parse --verify 'HEAD^{tree}')" \
    == "$authority_tree" \
  && "$(tagged_blob_sha256 "$source_root" "$authority_head" \
      ops/bootstrap-jankurai-root-seal.sh)" \
    == "$(sha256_file "$entrypoint_descriptor")" \
  && "$(tagged_blob_sha256 "$source_root" "$authority_head" \
      generated/jankurai-pin.env)" == "$(sha256_file "$pin_descriptor")" ]] ||
  fail 'installed entrypoint or pin differs from the authenticated Git blobs'

# The pin is sourced only from a retained installed descriptor after both its
# installed config digest and its exact published Git blob have been proven.
# shellcheck disable=SC1090
source "$pin_descriptor"
JERYU_JANKURAI_SOURCE_REPO="${JANKURAI_REPO:?source repo missing from pin}"
JERYU_JANKURAI_VERSION="${JANKURAI_VERSION:?version missing from pin}"
JERYU_JANKURAI_SHA256="${JANKURAI_BINARY_SHA256:?binary digest missing from pin}"
JERYU_JANKURAI_SOURCE_REV="${JANKURAI_REV:?source rev missing from pin}"
JERYU_JANKURAI_SOURCE_TAG="${JANKURAI_TAG:?source tag missing from pin}"
JERYU_JANKURAI_SOURCE_TREE="${JANKURAI_SOURCE_TREE:?source tree missing from pin}"
JERYU_JANKURAI_SOURCE_ARCHIVE_SHA256="${JANKURAI_SOURCE_ARCHIVE_SHA256:?archive digest missing from pin}"
JERYU_JANKURAI_CARGO_LOCK_SHA256="${JANKURAI_CARGO_LOCK_SHA256:?lock digest missing from pin}"
JERYU_JANKURAI_RUSTC_VERSION="${JANKURAI_RUSTC_VERSION:?rustc missing from pin}"
JERYU_JANKURAI_CARGO_VERSION="${JANKURAI_CARGO_VERSION:?cargo missing from pin}"
JERYU_JANKURAI_TARGET_TRIPLE="${JANKURAI_TARGET_TRIPLE:?target missing from pin}"
JERYU_JANKURAI_BUILD_MODE="${JANKURAI_BUILD_MODE:?build mode missing from pin}"
JERYU_JANKURAI_PACKAGE_PATH="${JANKURAI_PACKAGE_PATH:?package path missing from pin}"
JERYU_JANKURAI_BUILDER_IMAGE="${JANKURAI_BUILDER_IMAGE:?builder image missing from pin}"
JERYU_JANKURAI_BUILDER_IMAGE_ID="${JANKURAI_BUILDER_IMAGE_ID:?builder image id missing from pin}"
JERYU_JANKURAI_LINKER_VERSION="${JANKURAI_LINKER_VERSION:?linker missing from pin}"
JERYU_JANKURAI_GLIBC_VERSION="${JANKURAI_GLIBC_VERSION:?glibc missing from pin}"
JERYU_JANKURAI_VENDOR_FILES_SHA256="${JANKURAI_VENDOR_FILES_SHA256:?vendor digest missing from pin}"
JERYU_JANKURAI_VENDOR_FILE_COUNT="${JANKURAI_VENDOR_FILE_COUNT:?vendor count missing from pin}"
JERYU_JANKURAI_CARGO_CONFIG_SHA256="${JANKURAI_CARGO_CONFIG_SHA256:?cargo config missing from pin}"
JERYU_JANKURAI_BUILD_ENVIRONMENT="${JANKURAI_BUILD_ENVIRONMENT:?build environment missing from pin}"
JERYU_JANKURAI_RUSTFLAGS="${JANKURAI_RUSTFLAGS:?rustflags missing from pin}"
JERYU_JANKURAI_BUILD_COMMAND="${JANKURAI_BUILD_COMMAND:?build command missing from pin}"
JERYU_JANKURAI_BUILD_CONTEXT_SHA256="${JANKURAI_BUILD_CONTEXT_SHA256:?build context missing from pin}"
expected_candidate="$JERYU_JANKURAI_SHA256"
expected_version="$JERYU_JANKURAI_VERSION"
[[ "$expected_candidate" =~ ^[0-9a-f]{64}$ ]] ||
  fail 'candidate digest from pin authority is malformed'
if [[ "$test_mode" == 0 ]]; then
  [[ "$expected_candidate" == "$production_candidate_sha256" ]] ||
    fail 'production candidate digest is not the reviewed split.3 candidate'
fi

request_path="$1"
require_physical_file "$request_path" 'bootstrap request'
request_identity="$(file_identity "$request_path")"
exec {request_fd}<"$request_path"
request_descriptor="/proc/${BASHPID}/fd/${request_fd}"
[[ "$(file_identity "$request_descriptor")" == "$request_identity" ]] ||
  fail 'bootstrap request descriptor identity changed'
request_sha256="$(sha256_file "$request_descriptor")"

jq -e '
  select((keys | sort) == ([
    "attempt_id", "candidate_path", "candidate_receipt_path",
    "candidate_receipt_sha256", "created_at_epoch", "expires_at_epoch",
    "head_sha", "ref", "schema", "tree_sha"
  ] | sort))
  | select(.schema == "jeryu.jankurai-root-seal-bootstrap/v1")
  | select(.attempt_id | test("^[0-9a-f]{64}$"))
  | select(.ref | test("^refs/heads/codex/[A-Za-z0-9][A-Za-z0-9._/-]*[A-Za-z0-9]$"))
  | select((.ref | contains("..")) | not)
  | select((.ref | contains("//")) | not)
  | select((.ref | contains("@{")) | not)
  | select((.ref | endswith(".lock")) | not)
  | select(.head_sha | test("^[0-9a-f]{40}$"))
  | select(.tree_sha | test("^[0-9a-f]{40}$"))
  | select(.candidate_path | type == "string" and startswith("/"))
  | select(.candidate_receipt_path | type == "string" and startswith("/"))
  | select(.candidate_receipt_sha256 | test("^[0-9a-f]{64}$"))
  | select(.created_at_epoch | type == "number" and floor == .)
  | select(.expires_at_epoch | type == "number" and floor == .)
' "$request_descriptor" >/dev/null ||
  fail 'bootstrap request schema or closed fields are invalid'

attempt_id="$(jq -er '.attempt_id' "$request_descriptor")"
request_ref="$(jq -er '.ref' "$request_descriptor")"
head_sha="$(jq -er '.head_sha' "$request_descriptor")"
tree_sha="$(jq -er '.tree_sha' "$request_descriptor")"
candidate_path="$(jq -er '.candidate_path' "$request_descriptor")"
candidate_receipt_path="$(jq -er '.candidate_receipt_path' "$request_descriptor")"
candidate_receipt_sha256="$(jq -er '.candidate_receipt_sha256' "$request_descriptor")"
created_at_epoch="$(jq -er '.created_at_epoch' "$request_descriptor")"
expires_at_epoch="$(jq -er '.expires_at_epoch' "$request_descriptor")"
[[ "$request_ref" == "$control_ref" \
  && "$head_sha" == "$authority_head" \
  && "$tree_sha" == "$authority_tree" ]] ||
  fail 'bootstrap request differs from the installed reviewed authority'

(( expires_at_epoch >= created_at_epoch \
  && expires_at_epoch - created_at_epoch <= maximum_lifetime_seconds \
  && now_epoch >= created_at_epoch && now_epoch <= expires_at_epoch )) \
  || fail 'bootstrap request is expired, premature, or exceeds 900 seconds'
[[ "$candidate_receipt_sha256" != "$request_sha256" ]] ||
  fail 'candidate receipt and bootstrap request must be distinct'

require_physical_file "$candidate_path" 'qualified candidate'
candidate_identity="$(file_identity "$candidate_path")"
candidate_mode_links="$(stat -Lc '%a:%h' -- "$candidate_path")"
[[ "$candidate_mode_links" == '755:1' || "$candidate_mode_links" == '555:1' ]] \
  || fail 'qualified candidate must be mode 0755 or 0555 with one link'
exec {candidate_fd}<"$candidate_path"
candidate_descriptor="/proc/${BASHPID}/fd/${candidate_fd}"
[[ "$(file_identity "$candidate_descriptor")" == "$candidate_identity" ]] ||
  fail 'qualified candidate descriptor identity changed'
[[ "$(sha256_file "$candidate_descriptor")" == "$expected_candidate" ]] ||
  fail 'qualified candidate digest differs from the reviewed pin'

require_physical_file "$candidate_receipt_path" 'candidate qualification receipt'
receipt_identity="$(file_identity "$candidate_receipt_path")"
exec {receipt_fd}<"$candidate_receipt_path"
receipt_descriptor="/proc/${BASHPID}/fd/${receipt_fd}"
[[ "$(file_identity "$receipt_descriptor")" == "$receipt_identity" ]] ||
  fail 'candidate receipt descriptor identity changed'
[[ "$(basename "$candidate_receipt_path")" == \
  "${candidate_receipt_sha256}.json" ]] ||
  fail 'candidate receipt filename is not its declared content address'
[[ "$(sha256_file "$receipt_descriptor")" == "$candidate_receipt_sha256" ]] ||
  fail 'candidate receipt content address mismatch'

manifest_sha256="$(sha256_file "$source_root/tool-manifest.toml")"
[[ "$(sha256_file "$repo_root/tool-manifest.toml")" == "$manifest_sha256" ]] ||
  fail 'caller checkout manifest differs from authenticated source'
current_head="$(git -C "$repo_root" rev-parse --verify 'HEAD^{commit}')"
current_tree="$(git -C "$repo_root" rev-parse --verify 'HEAD^{tree}')"
current_branch="$(git -C "$repo_root" symbolic-ref --quiet --short HEAD)" ||
  fail 'Jeryu Tool bootstrap requires a named branch'
[[ "$(git -C "$repo_root" config --local --get remote.origin.url)" \
    == "$manifest_remote" ]] ||
  fail 'Jeryu Tool origin differs from bootstrap authority'
[[ -z "$(git -C "$repo_root" status --porcelain --untracked-files=all)" ]] ||
  fail 'Jeryu Tool checkout must be clean'
[[ "$request_ref" == "refs/heads/$current_branch" ]] ||
  fail 'bootstrap request ref differs from the checked-out branch'
[[ "$head_sha" == "$current_head" && "$tree_sha" == "$current_tree" ]] ||
  fail 'bootstrap request head or tree differs from the checkout'
"$splitctl_descriptor" jeryu-local ref-readback \
  --repo jeryu/jeryu-tool \
  --remote "$manifest_remote" \
  --ref "$control_ref" \
  --expected-head "$head_sha" \
  --token-file "$token_file" >/dev/null ||
  fail 'published bootstrap ref differs from the installed authority'

jq -e \
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
  --arg package_path "${JERYU_JANKURAI_PACKAGE_PATH}" \
  --arg builder_image "${JERYU_JANKURAI_BUILDER_IMAGE}" \
  --arg builder_image_id "${JERYU_JANKURAI_BUILDER_IMAGE_ID}" \
  --arg linker "${JERYU_JANKURAI_LINKER_VERSION}" \
  --arg glibc "${JERYU_JANKURAI_GLIBC_VERSION}" \
  --arg vendor "${JERYU_JANKURAI_VENDOR_FILES_SHA256}" \
  --arg vendor_count "${JERYU_JANKURAI_VENDOR_FILE_COUNT}" \
  --arg cargo_config "${JERYU_JANKURAI_CARGO_CONFIG_SHA256}" \
  --arg environment "${JERYU_JANKURAI_BUILD_ENVIRONMENT}" \
  --arg rustflags "${JERYU_JANKURAI_RUSTFLAGS}" \
  --arg command "${JERYU_JANKURAI_BUILD_COMMAND}" \
  --arg context "${JERYU_JANKURAI_BUILD_CONTEXT_SHA256}" \
  --arg digest "$expected_candidate" \
  --arg version "$expected_version" \
  --arg path "$candidate_path" \
  --arg manifest_repo "$manifest_remote" \
  --arg manifest_commit "$head_sha" \
  --arg manifest_tree "$tree_sha" \
  --arg manifest_sha "$manifest_sha256" '
  select((keys | sort) == ([
    "binary", "build", "conclusion", "governance", "installation",
    "operator", "run_id", "schema", "source", "test_mode", "timestamp"
  ] | sort))
  | select(.schema == "jeryu.jankurai-installation/v2")
  | select(.test_mode == true and .conclusion == "success")
  | select(.source.remote == $remote and .source.commit == $commit
    and .source.tag == $tag and .source.tree == $tree
    and .source.archive_sha256 == $archive
    and .source.cargo_lock_sha256 == $lock
    and .source.verification == "diagnostic-candidate")
  | select(.build.rustc == $rustc and .build.cargo == $cargo
    and .build.target_triple == $triple and .build.mode == $mode
    and .build.package_path == $package_path
    and .build.builder_image == $builder_image
    and .build.builder_image_id == $builder_image_id
    and .build.linker == $linker and .build.glibc == $glibc
    and .build.vendor_files_sha256 == $vendor
    and .build.vendor_file_count == $vendor_count
    and .build.cargo_config_sha256 == $cargo_config
    and .build.environment == $environment and .build.rustflags == $rustflags
    and .build.command == $command and .build.context_sha256 == $context
    and .build.cargo_net_offline == true and .build.closed_vendor == true
    and .build.network_none == true and .build.read_only_root == true
    and .build.non_root == true and .build.capabilities_dropped == true
    and .build.no_new_privileges == true
    and .build.container_engine_path == "/usr/bin/docker"
    and .build.git_global_config_disabled == true
    and .build.git_system_config_disabled == true
    and .build.git_http_follow_redirects == false
    and .build.git_terminal_prompt == false
    and .build.jankurai_update_check == false
    and .build.network_scope ==
      "local-forge-source-plus-closed-vendor-network-none"
    and .build.no_proxy == "127.0.0.1,localhost,::1")
  | select(.governance.status == "diagnostic-candidate"
    and .governance.manifest_repo == $manifest_repo
    and .governance.manifest_commit == $manifest_commit
    and .governance.manifest_tree == $manifest_tree
    and .governance.manifest_sha256 == $manifest_sha
    and .governance.protected_main == false
    and .governance.protection_policy == "not-applicable")
  | select(.binary.sha256 == $digest and .binary.version_output == $version)
  | select(.installation.path == $path and .installation.atomic == true)
' "$receipt_descriptor" >/dev/null ||
  fail 'candidate qualification receipt does not bind the exact candidate/ref/head/tree'

broker="$install_dir/jankurai"
publisher_config="$install_dir/host-ci-publisher.config.json"
sandbox_config="$install_dir/host-ci-sandbox.config.json"
install_identity="$(stat -Lc '%d:%i' -- "$install_dir")"
exec {install_fd}<"$install_dir"
install_descriptor="/proc/${BASHPID}/fd/${install_fd}"
[[ "$(stat -Lc '%d:%i' -- "$install_descriptor")" == "$install_identity" ]] ||
  fail 'host broker install directory descriptor identity changed'
require_physical_file "$broker" 'installed broker auditor'
require_physical_file "$publisher_config" 'publisher config'
require_physical_file "$sandbox_config" 'sandbox config'
[[ "$(stat -Lc '%u:%g:%a:%h' -- "$broker")" == '0:0:555:1' \
  || "$test_mode" == 1 ]] || fail 'production broker custody is not root:root 0555 single-link'
for config in "$publisher_config" "$sandbox_config"; do
  [[ "$(stat -Lc '%u:%g:%a:%h' -- "$config")" == '0:0:600:1' \
    || "$test_mode" == 1 ]] ||
    fail "production broker config custody is unsafe: $config"
done

open_authority_descriptors() {
  if [[ -n "${broker_fd:-}" ]]; then exec {broker_fd}<&-; fi
  if [[ -n "${publisher_fd:-}" ]]; then exec {publisher_fd}<&-; fi
  if [[ -n "${sandbox_fd:-}" ]]; then exec {sandbox_fd}<&-; fi
  broker_identity="$(file_identity "$broker")"
  publisher_identity="$(file_identity "$publisher_config")"
  sandbox_identity="$(file_identity "$sandbox_config")"
  exec {broker_fd}<"$broker"
  exec {publisher_fd}<"$publisher_config"
  exec {sandbox_fd}<"$sandbox_config"
  broker_descriptor="/proc/${BASHPID}/fd/${broker_fd}"
  publisher_descriptor="/proc/${BASHPID}/fd/${publisher_fd}"
  sandbox_descriptor="/proc/${BASHPID}/fd/${sandbox_fd}"
  [[ "$(file_identity "$broker_descriptor")" == "$broker_identity" \
    && "$(file_identity "$publisher_descriptor")" == "$publisher_identity" \
    && "$(file_identity "$sandbox_descriptor")" == "$sandbox_identity" ]] ||
    fail 'broker/config descriptor identity changed'
}
open_authority_descriptors
if [[ "$(sha256_file "$broker_descriptor")" != "$expected_predecessor" \
  || "$(jq -er '.jankurai_sha256' "$publisher_descriptor")" != "$expected_predecessor" \
  || "$(jq -er '.jankurai_sha256' "$sandbox_descriptor")" != "$expected_predecessor" ]]; then
  [[ -e "$state_root/active.json" && ! -L "$state_root/active.json" ]] ||
    fail 'broker/config authority differs from the protected predecessor without recovery state'
fi

mkdir -p -- "$state_root"
chmod 0700 -- "$state_root"
require_physical_dir "$state_root" 'bootstrap state root'
[[ "$(stat -Lc '%u:%a' -- "$state_root")" == "$(id -u):700" ]] ||
  fail 'bootstrap state root custody is unsafe'
mkdir -p -- "$runner_custody_root"
chmod 0711 -- "$runner_custody_root"
require_physical_dir "$runner_custody_root" 'runner custody root'
[[ "$(stat -Lc '%u:%a' -- "$runner_custody_root")" == "$(id -u):711" ]] ||
  fail 'runner custody root is unsafe'
if [[ "$test_mode" == 0 ]]; then
  [[ "$(stat -Lc '%u:%g:%a' -- "$runner_custody_root")" == '0:0:711' ]] ||
    fail 'production runner custody root is not root:root mode 0711'
fi
exec {lock_fd}>"$state_root/transaction.lock"
chmod 0600 "$state_root/transaction.lock"
flock -n "$lock_fd" || fail 'another root-seal bootstrap transaction holds custody'

atomic_install() {
  local source="$1" target="$2" mode="$3" expected_sha="$4"
  local target_name target_descriptor stage stage_identity
  [[ "$(stat -Lc '%d:%i' -- "$install_dir")" == "$install_identity" ]] ||
    fail 'host broker install directory public identity changed'
  target_name="$(basename "$target")"
  target_descriptor="$install_descriptor/$target_name"
  stage="$(mktemp "$install_descriptor/.${target_name}.bootstrap.XXXXXX")"
  cp -- "$source" "$stage"
  chmod "$mode" "$stage"
  [[ "$(sha256_file "$stage")" == "$expected_sha" ]] ||
    fail "staged digest mismatch for $target_name"
  stage_identity="$(file_identity "$stage")"
  durable_sync "$stage"
  mv -fT -- "$stage" "$target_descriptor"
  [[ "$(file_identity "$target_descriptor")" == "$stage_identity" \
    && "$(file_identity "$target")" == "$stage_identity" \
    && "$(sha256_file "$target_descriptor")" == "$expected_sha" ]] ||
    fail "atomic publication identity changed for $target_name"
  durable_sync "$install_descriptor"
}

active_file="$state_root/active.json"
restore_attempt() {
  local attempt="$1" reason="$2" attempt_dir meta predecessor_sha publisher_sha sandbox_sha
  local runner_attempt_dir
  attempt_dir="$state_root/attempts/$attempt"
  meta="$attempt_dir/restore.json"
  require_physical_file "$meta" 'bootstrap restore metadata'
  predecessor_sha="$(jq -er '.predecessor_sha256' "$meta")"
  publisher_sha="$(jq -er '.publisher_config_sha256' "$meta")"
  sandbox_sha="$(jq -er '.sandbox_config_sha256' "$meta")"
  [[ "$predecessor_sha" == "$expected_predecessor" ]] ||
    fail 'recovery predecessor differs from protected authority'
  for backup in jankurai.predecessor publisher.config.predecessor \
    sandbox.config.predecessor; do
    require_physical_file "$attempt_dir/$backup" "bootstrap recovery backup $backup"
  done
  [[ "$(sha256_file "$attempt_dir/jankurai.predecessor")" == "$predecessor_sha" \
    && "$(sha256_file "$attempt_dir/publisher.config.predecessor")" == "$publisher_sha" \
    && "$(sha256_file "$attempt_dir/sandbox.config.predecessor")" == "$sandbox_sha" ]] ||
    fail 'bootstrap recovery backups failed digest verification'
  atomic_install "$attempt_dir/jankurai.predecessor" "$broker" 0555 "$predecessor_sha"
  atomic_install "$attempt_dir/publisher.config.predecessor" \
    "$publisher_config" 0600 "$publisher_sha"
  atomic_install "$attempt_dir/sandbox.config.predecessor" \
    "$sandbox_config" 0600 "$sandbox_sha"
  [[ "$(jq -er '.jankurai_sha256' "$publisher_config")" == "$predecessor_sha" \
    && "$(jq -er '.jankurai_sha256' "$sandbox_config")" == "$predecessor_sha" ]] ||
    fail 'restored configs do not bind the protected predecessor'
  runner_attempt_dir="$runner_custody_root/$attempt"
  if [[ -e "$runner_attempt_dir" || -L "$runner_attempt_dir" ]]; then
    require_physical_dir "$runner_attempt_dir" 'root-held runner materialization'
    [[ "$(dirname "$(realpath -e -- "$runner_attempt_dir")")" \
      == "$runner_custody_root" ]] ||
      fail 'root-held runner materialization escaped its custody root'
    chmod 0700 "$runner_custody_root"
    chmod -R u+w "$runner_attempt_dir"
    rm -rf --one-file-system -- "$runner_attempt_dir"
    [[ ! -e "$runner_attempt_dir" && ! -L "$runner_attempt_dir" ]] ||
      fail 'root-held runner materialization cleanup failed'
    chmod 0711 "$runner_custody_root"
  fi
  rm -f -- "$active_file"
  durable_sync "$state_root"
  printf '%s\n' "$reason" >"$attempt_dir/restoration-status"
  chmod 0400 "$attempt_dir/restoration-status"
}

if [[ -e "$active_file" || -L "$active_file" ]]; then
  require_physical_file "$active_file" 'active bootstrap recovery marker'
  stale_attempt="$(jq -er '
    select((keys | sort) == ["attempt_id","request_sha256"])
    | select(.attempt_id | test("^[0-9a-f]{64}$"))
    | select(.request_sha256 | test("^[0-9a-f]{64}$"))
    | .attempt_id
  ' "$active_file")" || fail 'active bootstrap recovery marker is invalid'
  restore_attempt "$stale_attempt" 'interrupted-recovery'
  open_authority_descriptors
fi
[[ "$(sha256_file "$broker_descriptor")" == "$expected_predecessor" ]] ||
  fail 'installed broker is not the exact protected predecessor after recovery'
[[ "$(jq -er '.jankurai_sha256' "$publisher_descriptor")" == "$expected_predecessor" \
  && "$(jq -er '.jankurai_sha256' "$sandbox_descriptor")" == "$expected_predecessor" ]] ||
  fail 'broker configs do not bind the exact protected predecessor after recovery'

attempts_root="$state_root/attempts"
mkdir -p -- "$attempts_root"
chmod 0700 -- "$attempts_root"
heads_root="$state_root/heads"
mkdir -p -- "$heads_root"
chmod 0700 -- "$heads_root"
if [[ "${JERYU_BOOTSTRAP_TEST_ALLOW_HEAD_REUSE:-0}" != 1 ]]; then
  mkdir -m 0700 -- "$heads_root/$head_sha" 2>/dev/null ||
    fail 'this exact head already consumed its sole root-seal attempt'
fi
attempt_dir="$attempts_root/$attempt_id"
mkdir -m 0700 -- "$attempt_dir" 2>/dev/null ||
  fail 'bootstrap attempt identifier was already consumed'

cp -- "$candidate_descriptor" "$attempt_dir/jankurai.candidate"
chmod 0500 "$attempt_dir/jankurai.candidate"
require_physical_file "$attempt_dir/jankurai.candidate" 'root-held candidate'
[[ "$(stat -Lc '%u:%g:%a:%h' -- "$attempt_dir/jankurai.candidate")" \
    == "$(id -u):$(id -g):500:1" ]] ||
  fail 'root-held candidate custody is unsafe'
[[ "$(sha256_file "$attempt_dir/jankurai.candidate")" == "$expected_candidate" ]] ||
  fail 'root-held candidate staging digest changed'
[[ "$("$attempt_dir/jankurai.candidate" --version 2>/dev/null)" == "$expected_version" ]] ||
  fail 'root-held candidate version differs from the reviewed pin'
cp -- "$receipt_descriptor" "$attempt_dir/candidate-receipt.json"
chmod 0400 "$attempt_dir/candidate-receipt.json"
[[ "$(sha256_file "$attempt_dir/candidate-receipt.json")" == \
  "$candidate_receipt_sha256" ]] ||
  fail 'root-held candidate receipt staging digest changed'

cp -- "$broker_descriptor" "$attempt_dir/jankurai.predecessor"
cp -- "$publisher_descriptor" "$attempt_dir/publisher.config.predecessor"
cp -- "$sandbox_descriptor" "$attempt_dir/sandbox.config.predecessor"
chmod 0400 "$attempt_dir"/jankurai.predecessor \
  "$attempt_dir"/publisher.config.predecessor \
  "$attempt_dir"/sandbox.config.predecessor
publisher_config_sha="$(sha256_file "$attempt_dir/publisher.config.predecessor")"
sandbox_config_sha="$(sha256_file "$attempt_dir/sandbox.config.predecessor")"

result_stage="$attempt_dir/result.stage.json"
jq -n -S \
  --arg attempt_id "$attempt_id" \
  --arg predecessor "$expected_predecessor" \
  --arg publisher "$publisher_config_sha" \
  --arg sandbox "$sandbox_config_sha" \
  '{attempt_id:$attempt_id,predecessor_sha256:$predecessor,
    publisher_config_sha256:$publisher,sandbox_config_sha256:$sandbox}' \
  >"$attempt_dir/restore.json"
chmod 0400 "$attempt_dir/restore.json"
durable_sync "$attempt_dir"

jq --arg candidate "$expected_candidate" \
  '.jankurai_sha256 = $candidate' \
  "$attempt_dir/publisher.config.predecessor" \
  >"$attempt_dir/publisher.config.candidate"
jq --arg candidate "$expected_candidate" \
  '.jankurai_sha256 = $candidate' \
  "$attempt_dir/sandbox.config.predecessor" \
  >"$attempt_dir/sandbox.config.candidate"
chmod 0400 "$attempt_dir"/publisher.config.candidate \
  "$attempt_dir"/sandbox.config.candidate
jq -e --arg candidate "$expected_candidate" '
  .jankurai_sha256 == $candidate
' "$attempt_dir/publisher.config.candidate" >/dev/null ||
  fail 'candidate publisher config construction failed'
jq -e --arg candidate "$expected_candidate" '
  .jankurai_sha256 == $candidate
' "$attempt_dir/sandbox.config.candidate" >/dev/null ||
  fail 'candidate sandbox config construction failed'
jq -S 'del(.jankurai_sha256)' "$attempt_dir/publisher.config.predecessor" \
  >"$attempt_dir/publisher.predecessor.without-jankurai.json"
jq -S 'del(.jankurai_sha256)' "$attempt_dir/publisher.config.candidate" \
  >"$attempt_dir/publisher.candidate.without-jankurai.json"
cmp -s "$attempt_dir/publisher.predecessor.without-jankurai.json" \
  "$attempt_dir/publisher.candidate.without-jankurai.json" ||
  fail 'candidate publisher config changed authority beyond Jankurai digest'
jq -S 'del(.jankurai_sha256)' "$attempt_dir/sandbox.config.predecessor" \
  >"$attempt_dir/sandbox.predecessor.without-jankurai.json"
jq -S 'del(.jankurai_sha256)' "$attempt_dir/sandbox.config.candidate" \
  >"$attempt_dir/sandbox.candidate.without-jankurai.json"
cmp -s "$attempt_dir/sandbox.predecessor.without-jankurai.json" \
  "$attempt_dir/sandbox.candidate.without-jankurai.json" ||
  fail 'candidate sandbox config changed authority beyond Jankurai digest'
rm -f -- "$attempt_dir"/*.without-jankurai.json

active_stage="$(mktemp "$state_root/.active.XXXXXX")"
jq -n -S --arg attempt_id "$attempt_id" --arg request "$request_sha256" \
  '{attempt_id:$attempt_id,request_sha256:$request}' >"$active_stage"
chmod 0600 "$active_stage"
durable_sync "$active_stage"
mv -fT -- "$active_stage" "$active_file"
durable_sync "$state_root"

transaction_active=1
seal_pid=''
on_exit() {
  local exit_rc=$?
  trap - EXIT ERR HUP INT TERM
  if [[ "${transaction_active:-0}" == 1 ]]; then
    restore_attempt "$attempt_id" 'exit-trap-restoration' || exit_rc=1
    transaction_active=0
  fi
  cleanup_authority_scratch || exit_rc=1
  exit "$exit_rc"
}
on_signal() {
  local signal="$1"
  if [[ -n "${seal_pid:-}" ]]; then
    kill -TERM -- "-$seal_pid" 2>/dev/null || kill -TERM "$seal_pid" 2>/dev/null || true
    wait "$seal_pid" 2>/dev/null || true
  fi
  case "$signal" in
    HUP) exit 129 ;;
    INT) exit 130 ;;
    TERM) exit 143 ;;
  esac
}
trap on_exit EXIT
trap 'on_signal HUP' HUP
trap 'on_signal INT' INT
trap 'on_signal TERM' TERM

runner_attempt_dir="$runner_custody_root/$attempt_id"
mkdir -m 0700 -- "$runner_attempt_dir" 2>/dev/null ||
  fail 'runner custody identifier was already consumed'
assert_installed_authority_held
"$splitctl_descriptor" jeryu-local protection-readback \
  --repo veox/jain-split-ops \
  --required-check jain-split-ops/required \
  --token-file "$token_file" >/dev/null ||
  fail 'protected SplitOps main policy is not exact'
held_ops_root="$runner_attempt_dir/control-plane"
ops_materialization="$runner_attempt_dir/materialization.json"
"$splitctl_descriptor" jeryu-local git-materialize \
  --repo veox/jain-split-ops \
  --remote "$ops_remote" \
  --ref "$ops_ref" \
  --resolve-ref-head \
  --destination "$held_ops_root" \
  --token-file "$token_file" \
  --retain-exact-release-tag-ref "$ops_tag_ref" \
  --retain-exact-release-tag-commit "$ops_commit" \
  >"$ops_materialization" ||
  fail 'cannot authenticate protected SplitOps main and immutable release tag'
jq -e \
  --arg remote "$ops_remote" \
  --arg reference "$ops_ref" \
  --arg commit "$ops_commit" \
  --arg destination "$held_ops_root" \
  --arg tag "$ops_tag_ref" '
    select(.schema_version == "jain.jeryu-git-materialization/v1")
    | select(.repository == "veox/jain-split-ops")
    | select(.remote == $remote and .reference == $reference)
    | select(.commit == $commit and .destination == $destination)
    | select(.release_tag_ref == $tag and .release_tag_commit == $commit)
    | select(.origin_retained == false and .lfs_hydrated == false)
    | select(.status == "pass")
  ' "$ops_materialization" >/dev/null ||
  fail 'protected SplitOps materialization is not the exact installed release'
[[ "$(git -C "$held_ops_root" rev-parse --verify 'HEAD^{commit}')" \
    == "$ops_commit" \
  && "$(git -C "$held_ops_root" rev-parse --verify 'HEAD^{tree}')" \
    == "$(git -C "$held_ops_root" rev-parse --verify \
      "${ops_tag_ref}^{tree}")" ]] ||
  fail 'protected SplitOps main and immutable tag trees differ'
ops_runner_relative="ops/ci/split-host-ci.sh"
ops_parent_relative="ops/ci/split-host-ci-parent.sh"
ops_integrity_relative="ops/ci/host-ci-integrity.sh"
held_runner="$held_ops_root/$ops_runner_relative"
held_parent="$held_ops_root/$ops_parent_relative"
held_integrity="$held_ops_root/$ops_integrity_relative"
expected_runner_sha256="$(tagged_blob_sha256 "$held_ops_root" \
  "$ops_commit" "$ops_runner_relative")"
expected_parent_sha256="$(tagged_blob_sha256 "$held_ops_root" \
  "$ops_commit" "$ops_parent_relative")"
expected_integrity_sha256="$(tagged_blob_sha256 "$held_ops_root" \
  "$ops_commit" "$ops_integrity_relative")"
# The unprivileged parent asks host-ci-integrity for the authenticated main
# tracking ref. Materialization deliberately retains no remote or remote refs,
# so create only that exact local authority ref after both main and tag agree.
ops_tracking_ref="refs/remotes/origin/${ops_ref#refs/heads/}"
[[ -z "$(git -C "$held_ops_root" for-each-ref --format='%(refname)' \
  refs/remotes)" ]] ||
  fail 'protected SplitOps materialization retained remote-tracking refs'
git -c core.hooksPath=/dev/null -c core.fsmonitor=false \
  -C "$held_ops_root" update-ref --no-deref "$ops_tracking_ref" \
  "$ops_commit" "$(printf '0%.0s' {1..40})" ||
  fail 'cannot bind protected SplitOps parent tracking ref'
[[ "$(git -C "$held_ops_root" rev-parse --verify \
  "${ops_tracking_ref}^{commit}")" == "$ops_commit" ]] ||
  fail 'protected SplitOps parent tracking ref is not exact'
if [[ "$test_mode" == 0 ]]; then
  chown -R 0:0 -- "$runner_attempt_dir"
fi
find "$runner_attempt_dir" -type d -exec chmod 0555 {} +
find "$runner_attempt_dir" -type f -exec chmod 0444 {} +
chmod 0555 "$held_runner" "$held_parent" "$held_integrity"
require_physical_file "$held_runner" 'root-held host-CI runner'
require_physical_file "$held_parent" 'root-held host-CI parent helper'
require_physical_file "$held_integrity" 'root-held host-CI integrity helper'
if [[ "$test_mode" == 0 ]]; then
  [[ "$(stat -Lc '%u:%g:%a:%h' -- "$held_runner")" == '0:0:555:1' \
    && "$(stat -Lc '%u:%g:%a:%h' -- "$held_parent")" == '0:0:555:1' \
    && "$(stat -Lc '%u:%g:%a:%h' -- "$held_integrity")" \
      == '0:0:555:1' ]] ||
    fail 'root-held production runner custody is unsafe'
else
  [[ "$(stat -Lc '%u:%g:%a:%h' -- "$held_runner")" \
      == "$authority_uid:$authority_gid:555:1" \
    && "$(stat -Lc '%u:%g:%a:%h' -- "$held_parent")" \
      == "$authority_uid:$authority_gid:555:1" \
    && "$(stat -Lc '%u:%g:%a:%h' -- "$held_integrity")" \
      == "$authority_uid:$authority_gid:555:1" ]] ||
    fail 'root-held test runner custody is unsafe'
fi
held_runner_identity="$(file_identity "$held_runner")"
held_parent_identity="$(file_identity "$held_parent")"
held_integrity_identity="$(file_identity "$held_integrity")"
[[ "$(sha256_file "$held_runner")" == "$expected_runner_sha256" \
  && "$(sha256_file "$held_parent")" == "$expected_parent_sha256" \
  && "$(sha256_file "$held_integrity")" == "$expected_integrity_sha256" ]] ||
  fail 'root-held runner helpers differ from protected bytes'

if [[ "$test_mode" == 1 && \
  ( -n "${JERYU_BOOTSTRAP_TEST_PAUSE_READY_FILE:-}" \
    || -n "${JERYU_BOOTSTRAP_TEST_PAUSE_RELEASE_FILE:-}" ) ]]; then
  ready="${JERYU_BOOTSTRAP_TEST_PAUSE_READY_FILE:-}"
  release="${JERYU_BOOTSTRAP_TEST_PAUSE_RELEASE_FILE:-}"
  [[ -n "$ready" && -n "$release" ]] ||
    fail 'test pause requires both ready and release files'
  : >"$ready"
  while [[ ! -e "$release" ]]; do
    read -r -t 0.05 _ </dev/null || true
  done
fi

[[ "$(file_identity "$broker")" == "$broker_identity" \
  && "$(file_identity "$publisher_config")" == "$publisher_identity" \
  && "$(file_identity "$sandbox_config")" == "$sandbox_identity" ]] ||
  fail 'broker or config replacement detected before candidate publication'
assert_installed_authority_held
[[ "$(file_identity "$candidate_path")" == "$candidate_identity" ]] ||
  fail 'qualified candidate replacement detected before publication'
[[ "$(file_identity "$candidate_descriptor")" == "$candidate_identity" \
  && "$(sha256_file "$candidate_descriptor")" == "$expected_candidate" \
  && "$(sha256_file "$candidate_path")" == "$expected_candidate" ]] ||
  fail 'qualified candidate content drift detected before publication'
[[ "$(file_identity "$candidate_receipt_path")" == "$receipt_identity" \
  && "$(file_identity "$receipt_descriptor")" == "$receipt_identity" \
  && "$(sha256_file "$receipt_descriptor")" == "$candidate_receipt_sha256" \
  && "$(sha256_file "$candidate_receipt_path")" == "$candidate_receipt_sha256" ]] ||
  fail 'candidate receipt identity or content drift detected before publication'
[[ "$(file_identity "$held_runner")" == "$held_runner_identity" \
  && "$(file_identity "$held_parent")" == "$held_parent_identity" \
  && "$(file_identity "$held_integrity")" == "$held_integrity_identity" \
  && "$(sha256_file "$held_runner")" == "$expected_runner_sha256" \
  && "$(sha256_file "$held_parent")" == "$expected_parent_sha256" \
  && "$(sha256_file "$held_integrity")" == "$expected_integrity_sha256" \
  && "$(git -C "$held_ops_root" rev-parse --verify \
    "${ops_tracking_ref}^{commit}")" == "$ops_commit" ]] ||
  fail 'root-held host-CI runner custody changed before the attempt'
"$splitctl_descriptor" jeryu-local ref-readback \
  --repo jeryu/jeryu-tool \
  --remote "$manifest_remote" \
  --ref "$control_ref" \
  --expected-head "$head_sha" \
  --token-file "$token_file" >/dev/null ||
  fail 'published bootstrap ref changed before candidate publication'
"$splitctl_descriptor" jeryu-local protection-readback \
  --repo veox/jain-split-ops \
  --required-check jain-split-ops/required \
  --token-file "$token_file" >/dev/null ||
  fail 'protected SplitOps policy changed before the attempt'
"$splitctl_descriptor" jeryu-local ref-readback \
  --repo veox/jain-split-ops \
  --remote "$ops_remote" \
  --ref "$ops_ref" \
  --expected-head "$ops_commit" \
  --token-file "$token_file" >/dev/null ||
  fail 'protected SplitOps main changed before the attempt'
"$splitctl_descriptor" jeryu-local ref-readback \
  --repo veox/jain-split-ops \
  --remote "$ops_remote" \
  --ref "$ops_tag_ref" \
  --expected-head "$ops_commit" \
  --token-file "$token_file" >/dev/null ||
  fail 'immutable SplitOps release tag changed before the attempt'

candidate_publisher_sha="$(sha256_file "$attempt_dir/publisher.config.candidate")"
candidate_sandbox_sha="$(sha256_file "$attempt_dir/sandbox.config.candidate")"
atomic_install "$attempt_dir/jankurai.candidate" "$broker" 0555 "$expected_candidate"
atomic_install "$attempt_dir/publisher.config.candidate" \
  "$publisher_config" 0600 "$candidate_publisher_sha"
atomic_install "$attempt_dir/sandbox.config.candidate" \
  "$sandbox_config" 0600 "$candidate_sandbox_sha"
[[ "$(sha256_file "$broker")" == "$expected_candidate" \
  && "$(jq -er '.jankurai_sha256' "$publisher_config")" == "$expected_candidate" \
  && "$(jq -er '.jankurai_sha256' "$sandbox_config")" == "$expected_candidate" ]] \
  || fail 'candidate broker transaction did not become internally consistent'

set +e
if [[ "$test_mode" == 1 ]]; then
  (
    exec {lock_fd}>&-
    exec {request_fd}<&-
    exec {candidate_fd}<&-
    exec {receipt_fd}<&-
    exec {entrypoint_fd}<&-
    exec {authority_fd}<&-
    exec {splitops_config_fd}<&-
    exec {splitctl_fd}<&-
    exec {token_fd}<&-
    exec {pin_fd}<&-
    exec {install_fd}<&-
    exec {broker_fd}<&-
    exec {publisher_fd}<&-
    exec {sandbox_fd}<&-
    exec /usr/bin/setsid /usr/bin/setpriv --pdeathsig TERM -- \
      "$held_runner" jeryu jeryu-tool "$head_sha" "$repo_root" \
      jeryu-tool/required
  ) >"$attempt_dir/seal.log" 2>&1 &
else
  (
    exec {lock_fd}>&-
    exec {request_fd}<&-
    exec {candidate_fd}<&-
    exec {receipt_fd}<&-
    exec {entrypoint_fd}<&-
    exec {authority_fd}<&-
    exec {splitops_config_fd}<&-
    exec {splitctl_fd}<&-
    exec {token_fd}<&-
    exec {pin_fd}<&-
    exec {install_fd}<&-
    exec {broker_fd}<&-
    exec {publisher_fd}<&-
    exec {sandbox_fd}<&-
    exec /usr/bin/setsid /usr/bin/setpriv \
      --pdeathsig TERM --reuid=995 --regid=985 --clear-groups -- \
      /usr/bin/env -i \
        HOME=/var/lib/jain-host-ci-parent \
        PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
        LANG=C LC_ALL=C TZ=UTC JAIN_RELEASE_CI=1 \
        "$held_runner" jeryu jeryu-tool "$head_sha" "$repo_root" \
        jeryu-tool/required
  ) >"$attempt_dir/seal.log" 2>&1 &
fi
seal_pid=$!
wait "$seal_pid"
seal_rc=$?
seal_pid=''
set -e

restore_attempt "$attempt_id" 'completed-restoration'
transaction_active=0
completed_epoch="$(date +%s)"
jq -n -S \
  --arg attempt_id "$attempt_id" \
  --arg request_sha256 "$request_sha256" \
  --arg candidate_receipt_sha256 "$candidate_receipt_sha256" \
  --arg candidate_sha256 "$expected_candidate" \
  --arg runner_sha256 "$expected_runner_sha256" \
  --arg parent_sha256 "$expected_parent_sha256" \
  --arg integrity_sha256 "$expected_integrity_sha256" \
  --arg predecessor_sha256 "$expected_predecessor" \
  --arg ref "$control_ref" \
  --arg head "$head_sha" \
  --arg tree "$tree_sha" \
  --argjson created "$created_at_epoch" \
  --argjson expires "$expires_at_epoch" \
  --argjson completed "$completed_epoch" \
  --argjson seal_rc "$seal_rc" \
  '{schema:"jeryu.jankurai-root-seal-bootstrap-result/v1",
    attempt_id:$attempt_id,request_sha256:$request_sha256,
    candidate_receipt_sha256:$candidate_receipt_sha256,
    candidate_sha256:$candidate_sha256,runner_sha256:$runner_sha256,
    parent_sha256:$parent_sha256,integrity_sha256:$integrity_sha256,
    predecessor_sha256:$predecessor_sha256,
    ref:$ref,head_sha:$head,tree_sha:$tree,created_at_epoch:$created,
    expires_at_epoch:$expires,completed_at_epoch:$completed,seal_exit_code:$seal_rc,
    predecessor_restored:true,
    conclusion:(if $seal_rc == 0 then "success" else "failure" end)}' \
  >"$result_stage"
result_sha="$(sha256_file "$result_stage")"
result_path="$attempt_dir/$result_sha.json"
chmod 0400 "$result_stage" "$attempt_dir/seal.log"
mv -fT -- "$result_stage" "$result_path"
durable_sync "$attempt_dir"
printf 'bootstrap result=%s sha256=%s seal_exit_code=%s predecessor_restored=true\n' \
  "$result_path" "$result_sha" "$seal_rc"
exit "$seal_rc"
