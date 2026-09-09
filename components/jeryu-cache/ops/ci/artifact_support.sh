#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"
source ops/ci/lib.sh
jeryu_reject_ambient_git_authority 'artifact entrypoint source checkout' \
  || { printf 'artifact support failed: ambient Git authority rejected\n' >&2; exit 1; }
jeryu_reject_git_replacement_authority "$repo_root" \
  'artifact entrypoint source checkout' \
  || { printf 'artifact support failed: replacement-ref authority rejected\n' >&2; exit 1; }

artifact_root_relative='target/artifact-support'
artifact_root="$repo_root/$artifact_root_relative"
inventory_relative="$artifact_root_relative/jeryu-cache-release-inputs.sha256"
metadata_relative="$artifact_root_relative/cargo-metadata.json"
sbom_relative="$artifact_root_relative/jeryu-cache.spdx.json"
receipt_relative="$artifact_root_relative/jeryu-cache.json"
receipt="$repo_root/$receipt_relative"
release_version='jeryu-cache-v5.0.0-split.2'
syft_version_expected='1.40.0'
syft_sha256_expected='eb9714fb8e4b8f2a647e7bb312f1e0b9f83a7aa30418658bf46583cfa83d27d2'
local_tool_root='/var/lib/jain-host-ci/native-build-tools/36801d2417bbd9a804e09f30a8fd3ca96a9b5eb5e27ef9820143d44f07f4e240'
workspace_packages=(jeryu-cache jeryu-cache-adversary jeryu-cache-cli
  jeryu-cache-core jeryu-cache-service)
evidence_names=(cargo-metadata.json jeryu-cache-release-inputs.sha256
  jeryu-cache.json jeryu-cache.spdx.json)

fail() {
  printf 'artifact support failed: %s\n' "$*" >&2
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
  if [[ -e "$artifact_root" || -L "$artifact_root" ]]; then
    require_physical_directory "$artifact_root" 'artifact evidence root'
  else
    mkdir -- "$artifact_root"
    require_physical_directory "$artifact_root" 'artifact evidence root'
  fi
  if [[ -n "${evidence_root_fd:-}" ]]; then
    exec {evidence_root_fd}<&-
  fi
  if [[ -n "${evidence_parent_fd:-}" ]]; then
    exec {evidence_parent_fd}<&-
  fi
  exec {evidence_parent_fd}<"$repo_root/target" \
    || fail 'cannot hold physical target root'
  exec {evidence_root_fd}<"$artifact_root" \
    || fail 'cannot hold physical artifact evidence root'
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
    || fail 'artifact evidence descriptors are not initialized'
  parent_fd_identity="$(stat -Lc '%F|%d|%i|%u|%g|%a' -- \
    "$evidence_parent_exec" 2>/dev/null || true)"
  parent_path_identity="$(stat -Lc '%F|%d|%i|%u|%g|%a' -- \
    "$repo_root/target" 2>/dev/null || true)"
  root_fd_identity="$(stat -Lc '%F|%d|%i|%u|%g|%a' -- \
    "$evidence_root_exec" 2>/dev/null || true)"
  root_path_identity="$(stat -Lc '%F|%d|%i|%u|%g|%a' -- \
    "$artifact_root" 2>/dev/null || true)"
  [[ "$parent_fd_identity" == "$evidence_parent_identity" \
    && "$parent_path_identity" == "$evidence_parent_identity" \
    && "$root_fd_identity" == "$evidence_root_identity" \
    && "$root_path_identity" == "$evidence_root_identity" \
    && ! -L "$repo_root/target" && ! -L "$artifact_root" \
    && "$(realpath -e -- "$repo_root/target" 2>/dev/null || true)" \
      == "$repo_root/target" \
    && "$(realpath -e -- "$artifact_root" 2>/dev/null || true)" \
      == "$artifact_root" ]] \
    || fail 'canonical artifact evidence parent/root identity changed'
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
      || fail "artifact evidence root contains an unknown entry: $name"
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
    || fail 'artifact evidence root is not the exact closed four-file set'
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
  canonical="$artifact_root/$name"
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
  local expected_head="$1" expected_tree="$2" label="$3"
  local actual_head actual_tree dirty
  jeryu_assert_closed_source_authority "$repo_root" "$expected_head" "$label" \
    || fail "$label failed closed source authority validation"
  actual_head="$(jeryu_governed_git rev-parse 'HEAD^{commit}' \
    2>/dev/null || true)"
  actual_tree="$(jeryu_governed_git rev-parse 'HEAD^{tree}' \
    2>/dev/null || true)"
  dirty="$(jeryu_governed_git status --porcelain=v1 --untracked-files=all)"
  [[ "$actual_head" == "$expected_head" && "$actual_tree" == "$expected_tree" \
    && -z "$dirty" ]] \
    || fail "$label is not clean at the exact governed head and tree"
}

select_syft() {
  local actual_custody actual_version actual_sha path_identity kind links
  if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
    syft_bin='/opt/jain-ci/authority/security-bin/syft'
    syft_custody='release-read-only-security-bind'
  else
    syft_bin="$local_tool_root/bin/syft"
    syft_custody='local-root-owned-native-build-tools-v2'
  fi
  [[ "$syft_bin" == /* && -f "$syft_bin" && ! -L "$syft_bin" \
    && -x "$syft_bin" \
    && "$(realpath -e -- "$syft_bin" 2>/dev/null || true)" == "$syft_bin" ]] \
    || fail "Syft is not a physical absolute executable: $syft_bin"
  if [[ -n "${syft_fd:-}" ]]; then
    exec {syft_fd}<&-
  fi
  exec {syft_fd}<"$syft_bin" || fail "cannot hold Syft executable: $syft_bin"
  syft_exec="/proc/self/fd/$syft_fd"
  syft_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$syft_exec")"
  path_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$syft_bin")"
  IFS='|' read -r kind links _ <<< "$syft_identity"
  [[ "$kind" == 'regular file' && "$links" == 1 \
    && "$syft_identity" == "$path_identity" ]] \
    || fail 'Syft is not a held stable single-link executable'
  actual_sha="$(sha256sum -- "$syft_exec" | awk '{print $1}')"
  [[ "$actual_sha" == "$syft_sha256_expected" ]] \
    || fail "Syft SHA-256 mismatch at $syft_bin"
  actual_custody="$(stat -Lc '%u:%g:%a:%h' -- "$syft_bin")"
  require_tool_parent_custody "$syft_bin" 'Syft parent'
  if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
    [[ "$actual_custody" == '0:0:555:1' ]] \
      || fail "release Syft custody requires root:root mode 0555 nlink1: $actual_custody"
    require_read_only_mount "$syft_bin" 'release Syft'
  elif [[ "$actual_custody" != '0:0:555:1' \
    || "${actual_custody%%:*}" == "$EUID" || -w "$syft_bin" ]]; then
    fail "local Syft is not root-owned immutable custody: $actual_custody"
  fi
  actual_version="$("$syft_exec" version -o json | jq -er '
    select(keys == ["application","buildDate","compiler","gitCommit",
      "gitDescription","goVersion","platform","schemaVersion","version"])
    | select(.application == "syft") | .version')"
  [[ "$actual_version" == "$syft_version_expected" ]] \
    || fail "required Syft version is $syft_version_expected"
  assert_syft_stable
  syft_uid="${actual_custody%%:*}"
  actual_custody="${actual_custody#*:}"
  syft_gid="${actual_custody%%:*}"
  actual_custody="${actual_custody#*:}"
  syft_mode="${actual_custody%%:*}"
  syft_nlink="${actual_custody##*:}"
}

require_read_only_mount() {
  local path="$1" label="$2" options
  options="$(findmnt -rn -o OPTIONS --target "$path" 2>/dev/null || true)"
  [[ ",$options," == *,ro,* ]] || fail "$label is not on a read-only mount"
}

require_tool_parent_custody() {
  local path="$1" label="$2" parent custody
  parent="$(dirname -- "$path")"
  require_physical_directory "$parent" "$label"
  custody="$(stat -Lc '%u:%g:%a' -- "$parent")"
  [[ "$custody" == '0:0:555' && ! -w "$parent" ]] \
    || fail "$label is not root-owned mode 0555 non-writable custody: $custody"
}

assert_syft_stable() {
  local fd_identity path_identity digest
  [[ "$syft_bin" == /* && -f "$syft_bin" && ! -L "$syft_bin" \
    && -x "$syft_bin" \
    && "$(realpath -e -- "$syft_bin" 2>/dev/null || true)" == "$syft_bin" ]] \
    || fail 'canonical Syft path changed or became non-physical'
  fd_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$syft_exec")"
  path_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$syft_bin")"
  digest="$(sha256sum -- "$syft_exec" | awk '{print $1}')"
  [[ "$fd_identity" == "$syft_identity" && "$path_identity" == "$syft_identity" \
    && "$digest" == "$syft_sha256_expected" ]] \
    || fail 'held Syft identity changed or canonical path was swapped'
  if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
    require_read_only_mount "$syft_bin" 'release Syft'
    [[ "$(stat -Lc '%u:%g:%a:%h' -- "$syft_bin")" == '0:0:555:1' ]] \
      || fail 'release Syft lost root-owned immutable custody'
  else
    [[ "$(stat -Lc '%u:%g:%a:%h' -- "$syft_bin")" == '0:0:555:1' \
      && "$(stat -Lc '%u' -- "$syft_bin")" != "$EUID" \
      && ! -w "$syft_bin" ]] \
      || fail 'local Syft lost root-owned immutable custody'
  fi
  require_tool_parent_custody "$syft_bin" 'Syft parent'
}

write_inventory() {
  local destination="$1" expected_head="$2" path absolute digest
  : > "$destination"
  jeryu_governed_git ls-tree -rz --name-only "$expected_head" \
    | while IFS= read -r -d '' path; do
        if [[ "$path" == Cargo.lock || "$path" == Cargo.toml \
          || "$path" == VERSION || "$path" == rust-toolchain.toml \
          || "$path" == .cargo/* \
          || "$path" =~ ^crates/[^/]+/(Cargo\.toml|build\.rs)$ \
          || "$path" =~ ^crates/[^/]+/(src|tests)/.+\.rs$ \
          || "$path" == config/* || "$path" == policies/* ]]; then
          printf '%s\0' "$path"
        fi
      done \
    | LC_ALL=C sort -z \
    | while IFS= read -r -d '' path; do
        absolute="$repo_root/$path"
        digest="$(physical_file_digest "$absolute" "release input $path")"
        printf '%s  %s\n' "$digest" "$path"
      done > "$destination"
  [[ -s "$destination" ]] || fail 'release input inventory is empty'
  grep -Eq '^[0-9a-f]{64}  \.cargo/' "$destination" \
    || fail 'release inventory omits .cargo build configuration'
  grep -Eq '^[0-9a-f]{64}  rust-toolchain\.toml$' "$destination" \
    || fail 'release inventory omits rust-toolchain.toml'
  duplicate_count="$(cut -d' ' -f3- "$destination" | sort | uniq -d | wc -l | tr -d ' ')"
  [[ "$duplicate_count" == 0 ]] || fail 'release inventory contains duplicate paths'
}

validate_metadata() {
  local candidate="$1" expected_json
  expected_json="$(printf '%s\n' "${workspace_packages[@]}" | jq -R . | jq -sc .)"
  jq -e --arg root "$repo_root" --argjson expected "$expected_json" '
      (.packages | type == "array" and length == 5) and
      ([.packages[].name] | sort == $expected) and
      ([.packages[].name] | unique | length == 5) and
      all(.packages[];
        .version == "5.0.0" and
        (.manifest_path | startswith($root + "/crates/") and endswith("/Cargo.toml"))) and
      ((.workspace_members | type) == "array") and
      ((.workspace_members | length) == 5) and
      ((.workspace_members | unique | length) == 5) and
      (([.packages[].id] | sort) == (.workspace_members | sort))
    ' "$candidate" >/dev/null \
    || fail 'locked Cargo metadata does not match the exact five v5.0.0 workspace packages'
}

validate_receipt() {
  local candidate="$1" expected_head expected_tree expected_version count
  local receipt_digest receipt_digest_after inventory_digest metadata_digest sbom_digest
  local inventory_after metadata_after sbom_after expected_inventory
  local inventory_exec metadata_exec sbom_exec receipt_exec
  case "$candidate" in
    "$receipt_relative"|"$receipt") ;;
    *) fail "artifact receipt path is not canonical: $candidate" ;;
  esac
  ensure_evidence_root
  assert_exact_evidence_entries
  inventory_exec="$evidence_root_exec/jeryu-cache-release-inputs.sha256"
  metadata_exec="$evidence_root_exec/cargo-metadata.json"
  sbom_exec="$evidence_root_exec/jeryu-cache.spdx.json"
  receipt_exec="$evidence_root_exec/jeryu-cache.json"
  expected_head="$(jeryu_governed_git rev-parse 'HEAD^{commit}')"
  expected_tree="$(jeryu_governed_git rev-parse 'HEAD^{tree}')"
  assert_checkout_identity "$expected_head" "$expected_tree" \
    'artifact receipt source checkout'
  expected_version="$(tr -d '\n' < VERSION)"
  [[ "$expected_version" == "$release_version" ]] \
    || fail "VERSION does not name the governed successor $release_version"
  select_syft

  inventory_digest="$(evidence_file_digest \
    jeryu-cache-release-inputs.sha256 'artifact inventory')"
  metadata_digest="$(evidence_file_digest cargo-metadata.json \
    'Cargo metadata evidence')"
  sbom_digest="$(evidence_file_digest jeryu-cache.spdx.json 'SPDX evidence')"
  receipt_digest="$(evidence_file_digest jeryu-cache.json 'artifact receipt')"
  count="$(wc -l < "$inventory_exec" | tr -d ' ')"

  jq -e \
    --arg head "$expected_head" \
    --arg tree "$expected_tree" \
    --arg version "$expected_version" \
    --arg inventory "$inventory_relative" \
    --arg metadata "$metadata_relative" \
    --arg sbom "$sbom_relative" \
    --arg inventory_sha "$inventory_digest" \
    --arg metadata_sha "$metadata_digest" \
    --arg sbom_sha "$sbom_digest" \
    --arg syft_version "$syft_version_expected" \
    --arg syft_custody "$syft_custody" \
    --arg syft_path "$syft_bin" \
    --arg syft_sha "$syft_sha256_expected" \
    --arg syft_uid "$syft_uid" \
    --arg syft_gid "$syft_gid" \
    --arg syft_mode "$syft_mode" \
    --arg syft_nlink "$syft_nlink" \
    --argjson count "$count" '
      keys == ["artifact_count","cargo_metadata","cargo_metadata_sha256",
        "head_commit","head_tree","inventory","inventory_sha256",
        "production_applied","repo","sbom","sbom_sha256","schema_version",
        "status","syft_custody","syft_gid","syft_mode","syft_nlink","syft_path",
        "syft_sha256","syft_uid","syft_version","version"] and
      .schema_version == "jeryu.split.artifact-support/v3" and
      .repo == "jeryu-cache" and .version == $version and .status == "pass" and
      .head_commit == $head and .head_tree == $tree and
      .artifact_count == $count and .artifact_count > 0 and
      .inventory == $inventory and .inventory_sha256 == $inventory_sha and
      .cargo_metadata == $metadata and .cargo_metadata_sha256 == $metadata_sha and
      .sbom == $sbom and .sbom_sha256 == $sbom_sha and
      .syft_version == $syft_version and .syft_custody == $syft_custody and
      .syft_path == $syft_path and
      .syft_sha256 == $syft_sha and .syft_uid == $syft_uid and
      .syft_gid == $syft_gid and .syft_mode == $syft_mode and
      .syft_nlink == $syft_nlink and .production_applied == false
    ' "$receipt_exec" >/dev/null \
    || fail 'receipt envelope or physical-tool binding is invalid'

  validate_metadata "$metadata_exec"
  jq -e '
      .spdxVersion == "SPDX-2.3" and
      (.packages | type == "array" and length > 0)
    ' "$sbom_exec" >/dev/null || fail 'SPDX SBOM is empty or malformed'

  expected_inventory="$(mktemp "$repo_root/target/.artifact-inventory.XXXXXX")"
  write_inventory "$expected_inventory" "$expected_head"
  cmp -s -- "$expected_inventory" "$inventory_exec" || {
    rm -f -- "$expected_inventory"
    fail 'artifact inventory does not exactly bind the current clean source tree'
  }
  rm -f -- "$expected_inventory"

  inventory_after="$(evidence_file_digest \
    jeryu-cache-release-inputs.sha256 'artifact inventory')"
  metadata_after="$(evidence_file_digest cargo-metadata.json \
    'Cargo metadata evidence')"
  sbom_after="$(evidence_file_digest jeryu-cache.spdx.json 'SPDX evidence')"
  receipt_digest_after="$(evidence_file_digest jeryu-cache.json 'artifact receipt')"
  [[ "$inventory_after" == "$inventory_digest" \
    && "$metadata_after" == "$metadata_digest" \
    && "$sbom_after" == "$sbom_digest" \
    && "$receipt_digest_after" == "$receipt_digest" ]] \
    || fail 'artifact evidence changed during validation'
  assert_exact_evidence_entries
  assert_checkout_identity "$expected_head" "$expected_tree" \
    'artifact receipt source checkout after validation'
}

require_tool cargo
require_tool cmp
require_tool cut
require_tool find
require_tool findmnt
require_tool git
require_tool jq
require_tool realpath
require_tool sha256sum
require_tool stat
select_syft

if [[ "$#" == 2 && "$1" == --validate-receipt ]]; then
  validate_receipt "$2"
  printf 'artifact support receipt valid: %s\n' "$receipt_relative"
  exit 0
fi
[[ "$#" == 0 ]] \
  || fail 'usage: artifact_support.sh [--validate-receipt target/artifact-support/jeryu-cache.json]'

head_commit="$(jeryu_governed_git rev-parse 'HEAD^{commit}')"
head_tree="$(jeryu_governed_git rev-parse 'HEAD^{tree}')"
assert_checkout_identity "$head_commit" "$head_tree" 'artifact source checkout'
[[ "$(tr -d '\n' < VERSION)" == "$release_version" ]] \
  || fail "VERSION does not name the governed successor $release_version"

ensure_evidence_root
reject_unknown_evidence_entries
for previous_name in jeryu-cache-release-inputs.sha256 cargo-metadata.json \
  jeryu-cache.spdx.json jeryu-cache.json; do
  previous="$evidence_root_exec/$previous_name"
  if [[ -e "$previous" || -L "$previous" ]]; then
    evidence_file_digest "$previous_name" 'previous artifact evidence' >/dev/null
  fi
done
rm -f -- "$evidence_root_exec/jeryu-cache.json"

work_root="$(mktemp -d "$repo_root/target/.artifact-support.XXXXXX")"
cleanup() { rm -rf -- "$work_root"; }
trap cleanup EXIT

write_inventory "$work_root/inventory.sha256" "$head_commit"
assert_checkout_identity "$head_commit" "$head_tree" \
  'artifact source checkout after inventory generation'

command cargo metadata --locked --no-deps --format-version 1 \
  > "$work_root/cargo-metadata.json"
validate_metadata "$work_root/cargo-metadata.json"
assert_checkout_identity "$head_commit" "$head_tree" \
  'artifact source checkout after Cargo metadata'

assert_syft_stable
SYFT_CHECK_FOR_APP_UPDATE=false "$syft_exec" dir:. \
  --exclude './target/**' --exclude './.git/**' \
  --exclude './.jankurai/**' \
  --exclude './agent/repo-score.json' --exclude './agent/repo-score.md' \
  -o "spdx-json=$work_root/jeryu-cache.spdx.json" >/dev/null
assert_syft_stable
jq -e '.spdxVersion == "SPDX-2.3" and
  (.packages | type == "array" and length > 0)' \
  "$work_root/jeryu-cache.spdx.json" >/dev/null \
  || fail 'generated SPDX SBOM is empty or malformed'
assert_checkout_identity "$head_commit" "$head_tree" \
  'artifact source checkout after Syft execution'

mv -f -- "$work_root/inventory.sha256" \
  "$evidence_root_exec/jeryu-cache-release-inputs.sha256"
mv -f -- "$work_root/cargo-metadata.json" \
  "$evidence_root_exec/cargo-metadata.json"
mv -f -- "$work_root/jeryu-cache.spdx.json" \
  "$evidence_root_exec/jeryu-cache.spdx.json"
inventory_sha="$(evidence_file_digest \
  jeryu-cache-release-inputs.sha256 'artifact inventory')"
metadata_sha="$(evidence_file_digest cargo-metadata.json 'Cargo metadata evidence')"
sbom_sha="$(evidence_file_digest jeryu-cache.spdx.json 'SPDX evidence')"
version="$(tr -d '\n' < VERSION)"
artifact_count="$(wc -l \
  < "$evidence_root_exec/jeryu-cache-release-inputs.sha256" | tr -d ' ')"
receipt_tmp="$work_root/receipt.json"
jq -n \
  --arg repo jeryu-cache \
  --arg version "$version" \
  --arg head "$head_commit" \
  --arg tree "$head_tree" \
  --arg inventory "$inventory_relative" \
  --arg inventory_sha "$inventory_sha" \
  --arg metadata "$metadata_relative" \
  --arg metadata_sha "$metadata_sha" \
  --arg sbom "$sbom_relative" \
  --arg sbom_sha "$sbom_sha" \
  --arg syft_version "$syft_version_expected" \
  --arg syft_custody "$syft_custody" \
  --arg syft_path "$syft_bin" \
  --arg syft_sha "$syft_sha256_expected" \
  --arg syft_uid "$syft_uid" \
  --arg syft_gid "$syft_gid" \
  --arg syft_mode "$syft_mode" \
  --arg syft_nlink "$syft_nlink" \
  --argjson artifact_count "$artifact_count" '
    {schema_version:"jeryu.split.artifact-support/v3",repo:$repo,version:$version,
     status:"pass",head_commit:$head,head_tree:$tree,artifact_count:$artifact_count,
     inventory:$inventory,inventory_sha256:$inventory_sha,
     cargo_metadata:$metadata,cargo_metadata_sha256:$metadata_sha,
     sbom:$sbom,sbom_sha256:$sbom_sha,syft_version:$syft_version,
     syft_custody:$syft_custody,
     syft_path:$syft_path,syft_sha256:$syft_sha,syft_uid:$syft_uid,
     syft_gid:$syft_gid,syft_mode:$syft_mode,syft_nlink:$syft_nlink,
     production_applied:false}
  ' > "$receipt_tmp"
mv -f -- "$receipt_tmp" "$evidence_root_exec/jeryu-cache.json"
assert_exact_evidence_entries
assert_checkout_identity "$head_commit" "$head_tree" \
  'artifact source checkout before final validation'
validate_receipt "$receipt_relative"
printf 'artifact support ok: %s exact-tree inputs, receipt=%s\n' \
  "$artifact_count" "$receipt_relative"
