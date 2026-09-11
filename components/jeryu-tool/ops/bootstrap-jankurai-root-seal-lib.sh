#!/usr/bin/env bash
# Sourced only by bootstrap-jankurai-root-seal.sh.
[[ "${BASH_SOURCE[0]}" != "${0}" ]] || {
  printf 'bootstrap-jankurai-root-seal-lib: source from the entrypoint\n' >&2
  exit 1
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
