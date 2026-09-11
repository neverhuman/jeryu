#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"
source ops/ci/lib.sh
jeryu_reject_ambient_git_authority 'contract entrypoint source checkout' \
  || { printf 'contract drift failed: ambient Git authority rejected\n' >&2; exit 1; }
jeryu_reject_git_replacement_authority "$repo_root" \
  'contract entrypoint source checkout' \
  || { printf 'contract drift failed: replacement-ref authority rejected\n' >&2; exit 1; }

baseline_tag='jeryu-cache-v5.0.0-split.1'
baseline_commit='6bc56b87f051b8f74877f01f925fadc2e735b853'
tool_version='cargo-public-api 0.52.0'
tool_sha256='a903554dd723f83cb8fafb370c42cfb27b8eac1cbbd9f93b89d6ceaa33714798'
local_tool_root='/var/lib/jain-host-ci/native-build-tools/36801d2417bbd9a804e09f30a8fd3ca96a9b5eb5e27ef9820143d44f07f4e240'
release_tool_root='/opt/jain-ci/native-build-tools/36801d2417bbd9a804e09f30a8fd3ca96a9b5eb5e27ef9820143d44f07f4e240'
receipt_root_relative='target/contract-drift'
receipt_root="$repo_root/$receipt_root_relative"
receipt_relative="$receipt_root_relative/receipt.json"
receipt="$repo_root/$receipt_relative"
packages=(jeryu-cache-core jeryu-cache-service jeryu-cache-adversary jeryu-cache)
workspace_packages=(jeryu-cache jeryu-cache-adversary jeryu-cache-cli
  jeryu-cache-core jeryu-cache-service)
evidence_names=(receipt.json
  jeryu-cache-core.public-api.diff.json
  jeryu-cache-service.public-api.diff.json
  jeryu-cache-adversary.public-api.diff.json
  jeryu-cache.public-api.diff.json)

fail() {
  printf 'contract drift failed: %s\n' "$*" >&2
  exit 1
}

require_physical_directory() {
  local path="$1" label="$2" resolved
  [[ "$path" == /* && -d "$path" && ! -L "$path" ]] \
    || fail "$label is not a physical directory: $path"
  resolved="$(realpath -e -- "$path" 2>/dev/null || true)"
  [[ "$resolved" == "$path" ]] \
    || fail "$label path traverses a symlink: $path"
}

ensure_evidence_root() {
  if [[ -e "$repo_root/target" || -L "$repo_root/target" ]]; then
    require_physical_directory "$repo_root/target" 'target root'
  else
    mkdir -- "$repo_root/target"
    require_physical_directory "$repo_root/target" 'target root'
  fi
  if [[ -e "$receipt_root" || -L "$receipt_root" ]]; then
    require_physical_directory "$receipt_root" 'contract evidence root'
  else
    mkdir -- "$receipt_root"
    require_physical_directory "$receipt_root" 'contract evidence root'
  fi
  if [[ -n "${evidence_root_fd:-}" ]]; then
    exec {evidence_root_fd}<&-
  fi
  if [[ -n "${evidence_parent_fd:-}" ]]; then
    exec {evidence_parent_fd}<&-
  fi
  exec {evidence_parent_fd}<"$repo_root/target" \
    || fail 'cannot hold physical target root'
  exec {evidence_root_fd}<"$receipt_root" \
    || fail 'cannot hold physical contract evidence root'
  evidence_parent_exec="/proc/self/fd/$evidence_parent_fd"
  evidence_root_exec="/proc/self/fd/$evidence_root_fd"
  evidence_parent_identity="$(stat -Lc '%F|%d|%i|%u|%g|%a' -- \
    "$evidence_parent_exec")"
  evidence_root_identity="$(stat -Lc '%F|%d|%i|%u|%g|%a' -- \
    "$evidence_root_exec")"
  assert_evidence_root_stable
}

assert_evidence_root_stable() {
  local parent_fd_identity parent_path_identity root_fd_identity root_path_identity
  [[ "$evidence_parent_exec" == /proc/self/fd/* \
    && "$evidence_root_exec" == /proc/self/fd/* ]] \
    || fail 'contract evidence descriptors are not initialized'
  parent_fd_identity="$(stat -Lc '%F|%d|%i|%u|%g|%a' -- \
    "$evidence_parent_exec" 2>/dev/null || true)"
  parent_path_identity="$(stat -Lc '%F|%d|%i|%u|%g|%a' -- \
    "$repo_root/target" 2>/dev/null || true)"
  root_fd_identity="$(stat -Lc '%F|%d|%i|%u|%g|%a' -- \
    "$evidence_root_exec" 2>/dev/null || true)"
  root_path_identity="$(stat -Lc '%F|%d|%i|%u|%g|%a' -- \
    "$receipt_root" 2>/dev/null || true)"
  [[ "$parent_fd_identity" == "$evidence_parent_identity" \
    && "$parent_path_identity" == "$evidence_parent_identity" \
    && "$root_fd_identity" == "$evidence_root_identity" \
    && "$root_path_identity" == "$evidence_root_identity" \
    && ! -L "$repo_root/target" && ! -L "$receipt_root" \
    && "$(realpath -e -- "$repo_root/target" 2>/dev/null || true)" \
      == "$repo_root/target" \
    && "$(realpath -e -- "$receipt_root" 2>/dev/null || true)" \
      == "$receipt_root" ]] \
    || fail 'canonical contract evidence parent/root identity changed'
}

reject_unknown_evidence_entries() {
  local path name allowed candidate
  assert_evidence_root_stable
  while IFS= read -r -d '' path; do
    name="${path##*/}"
    allowed=false
    for candidate in "${evidence_names[@]}"; do
      [[ "$name" == "$candidate" ]] && allowed=true
    done
    [[ "$allowed" == true ]] \
      || fail "contract evidence root contains an unknown entry: $name"
  done < <(find -H "$evidence_root_exec" -mindepth 1 -maxdepth 1 -print0)
  assert_evidence_root_stable
}

assert_exact_evidence_entries() {
  local actual expected
  assert_evidence_root_stable
  actual="$(find -H "$evidence_root_exec" -mindepth 1 -maxdepth 1 -printf '%f\n' \
    | LC_ALL=C sort)"
  expected="$(printf '%s\n' "${evidence_names[@]}" | LC_ALL=C sort)"
  [[ "$actual" == "$expected" ]] \
    || fail 'contract evidence root is not the exact closed five-file set'
  assert_evidence_root_stable
}

remove_stale_evidence() {
  local package stale stale_target
  assert_evidence_root_stable
  for package in "${packages[@]}"; do
    stale="$evidence_root_exec/$package.public-api.diff.txt"
    if [[ -e "$stale" || -L "$stale" ]]; then
      evidence_file_digest "$package.public-api.diff.txt" \
        'stale contract report' >/dev/null
      rm -f -- "$stale"
    fi
  done
  stale_target="$evidence_root_exec/cargo-target"
  if [[ -e "$stale_target" || -L "$stale_target" ]]; then
    [[ -d "$stale_target" && ! -L "$stale_target" ]] \
      || fail 'stale contract build cache is not a physical directory'
    rm -rf -- "$stale_target"
  fi
  assert_evidence_root_stable
}

physical_file_digest() {
  local path="$1" label="$2" resolved fd before path_before after path_after
  local kind links digest
  [[ "$path" == /* && -f "$path" && ! -L "$path" ]] \
    || fail "$label is not a physical regular file: $path"
  resolved="$(realpath -e -- "$path" 2>/dev/null || true)"
  [[ "$resolved" == "$path" ]] \
    || fail "$label path traverses a symlink: $path"
  exec {fd}<"$path" || fail "cannot open $label: $path"
  before="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "/proc/self/fd/$fd")"
  path_before="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$path")"
  IFS='|' read -r kind links _ <<< "$before"
  [[ "$kind" == 'regular file' && "$links" == 1 && "$before" == "$path_before" ]] \
    || {
      exec {fd}<&-
      fail "$label is not a stable single-link regular file: $path"
    }
  digest="$(sha256sum -- "/proc/self/fd/$fd" | awk '{print $1}')"
  after="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "/proc/self/fd/$fd")"
  path_after="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$path")"
  exec {fd}<&-
  [[ "$before" == "$after" && "$before" == "$path_after" ]] \
    || fail "$label changed while it was read: $path"
  printf '%s\n' "$digest"
}

evidence_file_digest() {
  local name="$1" label="$2" canonical held resolved fd
  local before held_before canonical_before after held_after canonical_after
  local kind links digest
  [[ -n "$name" && "$name" != */* && "$name" != . && "$name" != .. ]] \
    || fail "$label has an invalid evidence filename: $name"
  assert_evidence_root_stable
  canonical="$receipt_root/$name"
  held="$evidence_root_exec/$name"
  [[ -f "$canonical" && ! -L "$canonical" ]] \
    || fail "$label is not a physical regular file: $canonical"
  resolved="$(realpath -e -- "$canonical" 2>/dev/null || true)"
  [[ "$resolved" == "$canonical" ]] \
    || fail "$label path traverses a symlink: $canonical"
  exec {fd}<"$held" || fail "cannot open held $label: $canonical"
  before="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- \
    "/proc/self/fd/$fd")"
  held_before="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$held")"
  canonical_before="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- \
    "$canonical")"
  IFS='|' read -r kind links _ <<< "$before"
  [[ "$kind" == 'regular file' && "$links" == 1 \
    && "$before" == "$held_before" && "$before" == "$canonical_before" ]] \
    || {
      exec {fd}<&-
      fail "$label is not a stable single-link file in the held evidence root"
    }
  digest="$(sha256sum -- "/proc/self/fd/$fd" | awk '{print $1}')"
  after="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- \
    "/proc/self/fd/$fd")"
  held_after="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$held")"
  canonical_after="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- \
    "$canonical")"
  exec {fd}<&-
  [[ "$before" == "$after" && "$before" == "$held_after" \
    && "$before" == "$canonical_after" ]] \
    || fail "$label or its canonical path changed while it was read"
  assert_evidence_root_stable
  printf '%s\n' "$digest"
}

assert_checkout_identity() {
  local checkout="$1" expected_head="$2" expected_tree="$3" label="$4"
  local actual_head actual_tree dirty
  jeryu_assert_closed_source_authority "$checkout" "$expected_head" "$label" \
    || fail "$label failed closed source authority validation"
  actual_head="$(jeryu_governed_git -C "$checkout" rev-parse 'HEAD^{commit}' \
    2>/dev/null || true)"
  actual_tree="$(jeryu_governed_git -C "$checkout" rev-parse 'HEAD^{tree}' \
    2>/dev/null || true)"
  dirty="$(jeryu_governed_git -C "$checkout" status \
    --porcelain=v1 --untracked-files=all)"
  [[ "$actual_head" == "$expected_head" && "$actual_tree" == "$expected_tree" \
    && -z "$dirty" ]] \
    || fail "$label is not clean at the exact governed head and tree"
}

validate_raw_report() {
  local path="$1" package="$2" raw prefix added
  [[ -s "$path" ]] || fail "public API report is empty for $package"
  if LC_ALL=C grep -q '[^[:print:][:space:]]' "$path"; then
    fail "public API report contains non-text bytes for $package"
  fi
  raw="$(<"$path")"
  prefix=$'Removed items from the public API\n=================================\n(none)\n\nChanged items in the public API\n===============================\n(none)\n\nAdded items to the public API\n=============================\n'
  [[ "$raw" == "$prefix"* ]] \
    || fail "public API report is empty or unstructured for $package"
  added="${raw#"$prefix"}"
  [[ -n "$added" && "$added" =~ [^[:space:]] ]] \
    || fail "public API added-items section is empty for $package"
  [[ "$added" != *'Removed items from the public API'* \
    && "$added" != *'Changed items in the public API'* \
    && "$added" != *'Added items to the public API'* ]] \
    || fail "public API report repeats a structural section for $package"
  if [[ "$added" == '(none)' ]]; then
    printf '%s\n' compatible-no-diff
  else
    [[ "$added" != *'(none)'* ]] \
      || fail "public API added-items section mixes none with content for $package"
    printf '%s\n' compatible-additive-only
  fi
}

select_cargo_public_api() {
  local actual_custody actual_version actual_sha path_identity kind links
  if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
    cargo_public_api_bin="$release_tool_root/bin/cargo-public-api"
    tool_custody='release-read-only-native-build-tools-v2'
  else
    cargo_public_api_bin="$local_tool_root/bin/cargo-public-api"
    tool_custody='local-root-owned-native-build-tools-v2'
  fi
  [[ "$cargo_public_api_bin" == /* && -f "$cargo_public_api_bin" \
    && ! -L "$cargo_public_api_bin" && -x "$cargo_public_api_bin" \
    && "$(realpath -e -- "$cargo_public_api_bin" 2>/dev/null || true)" \
      == "$cargo_public_api_bin" ]] \
    || fail "cargo-public-api is not a physical absolute executable: $cargo_public_api_bin"
  if [[ -n "${cargo_public_api_fd:-}" ]]; then
    exec {cargo_public_api_fd}<&-
  fi
  exec {cargo_public_api_fd}<"$cargo_public_api_bin" \
    || fail "cannot hold cargo-public-api executable: $cargo_public_api_bin"
  cargo_public_api_exec="/proc/self/fd/$cargo_public_api_fd"
  cargo_public_api_identity="$(stat -Lc \
    '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$cargo_public_api_exec")"
  path_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- \
    "$cargo_public_api_bin")"
  IFS='|' read -r kind links _ <<< "$cargo_public_api_identity"
  [[ "$kind" == 'regular file' && "$links" == 1 \
    && "$cargo_public_api_identity" == "$path_identity" ]] \
    || fail "cargo-public-api is not a held stable single-link executable"
  actual_sha="$(sha256sum -- "$cargo_public_api_exec" | awk '{print $1}')"
  [[ "$actual_sha" == "$tool_sha256" ]] \
    || fail "cargo-public-api SHA-256 mismatch at $cargo_public_api_bin"
  actual_custody="$(stat -Lc '%u:%g:%a:%h' -- "$cargo_public_api_bin")"
  require_tool_parent_custody "$cargo_public_api_bin" 'cargo-public-api parent'
  if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
    [[ "$actual_custody" == '0:0:555:1' ]] \
      || fail "release cargo-public-api custody requires root:root mode 0555 nlink1: $actual_custody"
    require_read_only_mount "$cargo_public_api_bin" 'release cargo-public-api'
  elif [[ "$actual_custody" != '0:0:555:1' \
    || "${actual_custody%%:*}" == "$EUID" || -w "$cargo_public_api_bin" ]]; then
    fail "local cargo-public-api is not root-owned immutable custody: $actual_custody"
  fi
  actual_version="$("$cargo_public_api_exec" --version 2>/dev/null || true)"
  [[ "$actual_version" == "$tool_version" ]] \
    || fail "required tool identity is $tool_version"
  assert_cargo_public_api_stable
  tool_uid="${actual_custody%%:*}"
  actual_custody="${actual_custody#*:}"
  tool_gid="${actual_custody%%:*}"
  actual_custody="${actual_custody#*:}"
  tool_mode="${actual_custody%%:*}"
  tool_nlink="${actual_custody##*:}"
}

require_read_only_mount() {
  local path="$1" label="$2" options
  options="$(findmnt -rn -o OPTIONS --target "$path" 2>/dev/null || true)"
  [[ ",$options," == *,ro,* ]] \
    || fail "$label is not on a read-only mount"
}

require_tool_parent_custody() {
  local path="$1" label="$2" parent custody
  parent="$(dirname -- "$path")"
  require_physical_directory "$parent" "$label"
  custody="$(stat -Lc '%u:%g:%a' -- "$parent")"
  [[ "$custody" == '0:0:555' && ! -w "$parent" ]] \
    || fail "$label is not root-owned mode 0555 non-writable custody: $custody"
}

assert_cargo_public_api_stable() {
  local fd_identity path_identity digest
  [[ "$cargo_public_api_bin" == /* && -f "$cargo_public_api_bin" \
    && ! -L "$cargo_public_api_bin" && -x "$cargo_public_api_bin" \
    && "$(realpath -e -- "$cargo_public_api_bin" 2>/dev/null || true)" \
      == "$cargo_public_api_bin" ]] \
    || fail 'canonical cargo-public-api path changed or became non-physical'
  fd_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- \
    "$cargo_public_api_exec")"
  path_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- \
    "$cargo_public_api_bin")"
  digest="$(sha256sum -- "$cargo_public_api_exec" | awk '{print $1}')"
  [[ "$fd_identity" == "$cargo_public_api_identity" \
    && "$path_identity" == "$cargo_public_api_identity" \
    && "$digest" == "$tool_sha256" ]] \
    || fail 'held cargo-public-api identity changed or canonical path was swapped'
  if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
    require_read_only_mount "$cargo_public_api_bin" 'release cargo-public-api'
    [[ "$(stat -Lc '%u:%g:%a:%h' -- "$cargo_public_api_bin")" == '0:0:555:1' ]] \
      || fail 'release cargo-public-api lost root-owned immutable custody'
  else
    [[ "$(stat -Lc '%u:%g:%a:%h' -- "$cargo_public_api_bin")" == '0:0:555:1' \
      && "$(stat -Lc '%u' -- "$cargo_public_api_bin")" != "$EUID" \
      && ! -w "$cargo_public_api_bin" ]] \
      || fail 'local cargo-public-api lost root-owned immutable custody'
  fi
  require_tool_parent_custody "$cargo_public_api_bin" 'cargo-public-api parent'
}

validate_normalized_report() {
  local name="$1" package="$2" expected_head="$3" expected_digest="$4"
  local path="$evidence_root_exec/$name"
  local digest raw_tmp raw_sha raw_bytes result expected_result
  digest="$(evidence_file_digest "$name" "normalized API evidence for $package")"
  [[ "$digest" == "$expected_digest" ]] \
    || fail "normalized API evidence digest mismatch for $package"
  jq -e \
    --arg package "$package" \
    --arg baseline "$baseline_commit" \
    --arg head "$expected_head" '
      keys == ["baseline_commit","head_commit","package","raw_report",
        "raw_report_bytes","raw_report_sha256","result","schema_version","status"] and
      .schema_version == "jeryu.cache.public-api-diff/v1" and
      .status == "pass" and .package == $package and
      .baseline_commit == $baseline and .head_commit == $head and
      (.raw_report | type == "string" and length > 0) and
      (.raw_report_bytes | type == "number" and . > 0 and floor == .) and
      (.raw_report_sha256 | test("^[0-9a-f]{64}$")) and
      (.result == "compatible-no-diff" or .result == "compatible-additive-only")
    ' "$path" >/dev/null \
    || fail "normalized API evidence is empty or malformed for $package"
  raw_tmp="$(mktemp "$repo_root/target/.contract-report.XXXXXX")"
  jq -j '.raw_report' "$path" > "$raw_tmp"
  raw_sha="$(sha256sum -- "$raw_tmp" | awk '{print $1}')"
  raw_bytes="$(wc -c < "$raw_tmp" | tr -d ' ')"
  [[ "$raw_sha" == "$(jq -er '.raw_report_sha256' "$path")" \
    && "$raw_bytes" == "$(jq -er '.raw_report_bytes' "$path")" ]] \
    || {
      rm -f -- "$raw_tmp"
      fail "normalized API report binding is invalid for $package"
    }
  expected_result="$(validate_raw_report "$raw_tmp" "$package")"
  result="$(jq -er '.result' "$path")"
  rm -f -- "$raw_tmp"
  [[ "$result" == "$expected_result" ]] \
    || fail "normalized API result is inconsistent for $package"
  [[ "$(evidence_file_digest "$name" "normalized API evidence for $package")" \
    == "$digest" ]] \
    || fail "normalized API evidence changed during validation for $package"
}

validate_receipt() {
  local candidate="$1" expected_head expected_tree config_sha policy_sha
  local receipt_exec receipt_digest receipt_digest_after package output_name digest
  case "$candidate" in
    "$receipt_relative"|"$receipt") ;;
    *) fail "contract receipt path is not canonical: $candidate" ;;
  esac
  ensure_evidence_root
  assert_exact_evidence_entries
  receipt_exec="$evidence_root_exec/receipt.json"
  expected_head="$(jeryu_governed_git rev-parse 'HEAD^{commit}')"
  expected_tree="$(jeryu_governed_git rev-parse 'HEAD^{tree}')"
  assert_checkout_identity "$repo_root" "$expected_head" "$expected_tree" \
    'contract receipt source checkout'
  select_cargo_public_api
  config_sha="$(physical_file_digest \
    "$repo_root/config/jeryu-cache-policy.toml" 'cache configuration contract')"
  policy_sha="$(physical_file_digest \
    "$repo_root/policies/cache-laws.toml" 'cache policy contract')"
  receipt_digest="$(evidence_file_digest receipt.json 'contract receipt')"
  packages_json="$(printf '%s\n' "${packages[@]}" | jq -R . | jq -sc .)"
  jq -e \
    --arg baseline_tag "$baseline_tag" \
    --arg baseline_commit "$baseline_commit" \
    --arg head "$expected_head" \
    --arg tree "$expected_tree" \
    --arg tool "$tool_version" \
    --arg tool_custody "$tool_custody" \
    --arg tool_path "$cargo_public_api_bin" \
    --arg tool_sha "$tool_sha256" \
    --arg tool_uid "$tool_uid" \
    --arg tool_gid "$tool_gid" \
    --arg tool_mode "$tool_mode" \
    --arg tool_nlink "$tool_nlink" \
    --arg config_sha "$config_sha" \
    --arg policy_sha "$policy_sha" \
    --argjson packages "$packages_json" '
      keys == ["baseline_commit","baseline_tag","config_sha256","contract_role",
        "head_commit","head_tree","policy_sha256","public_api","schema_version",
        "status","tool","tool_custody","tool_gid","tool_mode","tool_nlink","tool_path",
        "tool_sha256","tool_uid"] and
      .schema_version == "jeryu.cache.contract-drift/v2" and .status == "pass" and
      .contract_role == "cache-public-rust-api-and-policy" and
      .baseline_tag == $baseline_tag and .baseline_commit == $baseline_commit and
      .head_commit == $head and .head_tree == $tree and .tool == $tool and
      .tool_custody == $tool_custody and
      .tool_path == $tool_path and .tool_sha256 == $tool_sha and
      .tool_uid == $tool_uid and .tool_gid == $tool_gid and
      .tool_mode == $tool_mode and .tool_nlink == $tool_nlink and
      .config_sha256 == $config_sha and .policy_sha256 == $policy_sha and
      (.public_api | type == "array" and length == 4) and
      ([.public_api[].package] == $packages) and
      ([.public_api[].package] | unique | length == 4) and
      all(.public_api[];
        .output == ("target/contract-drift/" + .package + ".public-api.diff.json") and
        (.sha256 | test("^[0-9a-f]{64}$")))
    ' "$receipt_exec" >/dev/null || fail 'closed contract receipt validation failed'

  for package in "${packages[@]}"; do
    output_name="$package.public-api.diff.json"
    digest="$(jq -er --arg package "$package" \
      '.public_api[] | select(.package == $package) | .sha256' "$receipt_exec")"
    validate_normalized_report "$output_name" "$package" "$expected_head" "$digest"
  done
  receipt_digest_after="$(evidence_file_digest receipt.json 'contract receipt')"
  [[ "$receipt_digest_after" == "$receipt_digest" ]] \
    || fail 'contract receipt changed during validation'
  assert_exact_evidence_entries
  assert_checkout_identity "$repo_root" "$expected_head" "$expected_tree" \
    'contract receipt source checkout after validation'
}

require_tool cargo
require_tool find
require_tool findmnt
require_tool git
require_tool grep
require_tool jq
require_tool realpath
require_tool sha256sum
require_tool stat
select_cargo_public_api

if [[ "$#" == 2 && "$1" == --validate-receipt ]]; then
  validate_receipt "$2"
  printf 'contract drift receipt valid: %s\n' "$receipt_relative"
  exit 0
fi
[[ "$#" == 0 ]] \
  || fail 'usage: contract-drift.sh [--validate-receipt target/contract-drift/receipt.json]'

[[ "$(jeryu_governed_git rev-parse "$baseline_tag^{commit}" \
  2>/dev/null || true)" == "$baseline_commit" ]] \
  || fail 'immutable baseline tag does not resolve to the governed split.1 commit'
jeryu_governed_git merge-base --is-ancestor "$baseline_commit" HEAD \
  || fail 'HEAD is not descended from the governed contract baseline'

head_commit="$(jeryu_governed_git rev-parse 'HEAD^{commit}')"
head_tree="$(jeryu_governed_git rev-parse 'HEAD^{tree}')"
assert_checkout_identity "$repo_root" "$head_commit" "$head_tree" \
  'contract source checkout'
jeryu_governed_git diff --quiet "$baseline_commit" HEAD -- \
  .cargo rust-toolchain.toml config/jeryu-cache-policy.toml policies/cache-laws.toml \
  || fail 'build, policy, or configuration contract changed without a versioned successor'

ensure_evidence_root
remove_stale_evidence
reject_unknown_evidence_entries
if [[ -e "$receipt" || -L "$receipt" ]]; then
  evidence_file_digest receipt.json 'previous contract receipt' >/dev/null
  rm -f -- "$evidence_root_exec/receipt.json"
fi

work_root="$(mktemp -d "$repo_root/target/.contract-drift.XXXXXX")"
sandbox="$work_root/checkout"
rows="$work_root/rows.tsv"
metadata="$work_root/cargo-metadata.json"
contract_target="$repo_root/target/contract-drift-cargo-target"
cleanup() { rm -rf -- "$work_root"; }
trap cleanup EXIT
: > "$rows"

jeryu_governed_git clone --quiet --no-local "$repo_root" "$sandbox"
assert_checkout_identity "$sandbox" "$head_commit" "$head_tree" \
  'isolated contract sandbox'

(cd "$sandbox" && command cargo metadata --locked --no-deps --format-version 1) \
  > "$metadata"
workspace_packages_json="$(printf '%s\n' "${workspace_packages[@]}" | jq -R . | jq -sc .)"
jq -e --argjson expected "$workspace_packages_json" '
    (.packages | type == "array" and length == 5) and
    ([.packages[].name] | sort == $expected) and
    ([.packages[].name] | unique | length == 5) and
    ((.workspace_members | type) == "array") and
    ((.workspace_members | length) == 5) and
    ((.workspace_members | unique | length) == 5) and
    (([.packages[].id] | sort) == (.workspace_members | sort))
  ' "$metadata" >/dev/null \
  || fail 'Cargo metadata does not expose the exact unique five-package workspace'
assert_checkout_identity "$sandbox" "$head_commit" "$head_tree" \
  'contract sandbox after Cargo metadata'

for package in "${packages[@]}"; do
  raw="$work_root/$package.raw.txt"
  normalized="$work_root/$package.normalized.json"
  output_relative="$receipt_root_relative/$package.public-api.diff.json"
  output_name="$package.public-api.diff.json"
  assert_checkout_identity "$sandbox" "$head_commit" "$head_tree" \
    "contract sandbox before $package"
  assert_cargo_public_api_stable
  if ! (cd "$sandbox" && CARGO_NET_OFFLINE=true \
    CARGO_TARGET_DIR="$contract_target" "$cargo_public_api_exec" -p "$package" \
      diff "$baseline_commit..$head_commit" --deny removed --deny changed) \
      > "$raw"; then
    fail "public API drift rejected for $package"
  fi
  assert_cargo_public_api_stable
  result="$(validate_raw_report "$raw" "$package")"
  assert_checkout_identity "$sandbox" "$head_commit" "$head_tree" \
    "contract sandbox after $package"
  raw_sha="$(sha256sum -- "$raw" | awk '{print $1}')"
  raw_bytes="$(wc -c < "$raw" | tr -d ' ')"
  jq -n \
    --arg package "$package" \
    --arg baseline "$baseline_commit" \
    --arg head "$head_commit" \
    --arg result "$result" \
    --arg raw_sha "$raw_sha" \
    --argjson raw_bytes "$raw_bytes" \
    --rawfile raw_report "$raw" '
      {schema_version:"jeryu.cache.public-api-diff/v1",status:"pass",
       package:$package,baseline_commit:$baseline,head_commit:$head,result:$result,
       raw_report_sha256:$raw_sha,raw_report_bytes:$raw_bytes,
       raw_report:$raw_report}
    ' > "$normalized"
  mv -f -- "$normalized" "$evidence_root_exec/$output_name"
  digest="$(evidence_file_digest "$output_name" \
    "normalized API evidence for $package")"
  validate_normalized_report "$output_name" "$package" "$head_commit" "$digest"
  printf '%s\t%s\t%s\n' "$package" "$output_relative" "$digest" >> "$rows"
done

public_api="$(jq -Rn \
  '[inputs | split("\t") | {package:.[0],output:.[1],sha256:.[2]}]' < "$rows")"
config_sha="$(physical_file_digest \
  "$repo_root/config/jeryu-cache-policy.toml" 'cache configuration contract')"
policy_sha="$(physical_file_digest \
  "$repo_root/policies/cache-laws.toml" 'cache policy contract')"
receipt_tmp="$work_root/receipt.json"
jq -n \
  --arg baseline_tag "$baseline_tag" \
  --arg baseline_commit "$baseline_commit" \
  --arg head "$head_commit" \
  --arg tree "$head_tree" \
  --arg tool "$tool_version" \
  --arg tool_custody "$tool_custody" \
  --arg tool_path "$cargo_public_api_bin" \
  --arg tool_sha "$tool_sha256" \
  --arg tool_uid "$tool_uid" \
  --arg tool_gid "$tool_gid" \
  --arg tool_mode "$tool_mode" \
  --arg tool_nlink "$tool_nlink" \
  --arg config_sha "$config_sha" \
  --arg policy_sha "$policy_sha" \
  --argjson public_api "$public_api" '
    {schema_version:"jeryu.cache.contract-drift/v2",status:"pass",
     contract_role:"cache-public-rust-api-and-policy",baseline_tag:$baseline_tag,
     baseline_commit:$baseline_commit,head_commit:$head,head_tree:$tree,tool:$tool,
     tool_custody:$tool_custody,
     tool_path:$tool_path,tool_sha256:$tool_sha,tool_uid:$tool_uid,
     tool_gid:$tool_gid,tool_mode:$tool_mode,tool_nlink:$tool_nlink,
     config_sha256:$config_sha,policy_sha256:$policy_sha,public_api:$public_api}
  ' > "$receipt_tmp"
mv -f -- "$receipt_tmp" "$evidence_root_exec/receipt.json"
assert_exact_evidence_entries
assert_checkout_identity "$repo_root" "$head_commit" "$head_tree" \
  'contract source checkout after public API execution'
validate_receipt "$receipt_relative"

printf 'contract drift ok: exact four-package public APIs and policy/build baseline, receipt=%s\n' \
  "$receipt_relative"
