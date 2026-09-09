#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
baseline_tag='jeryu-cache-v5.0.0-split.1'
baseline_commit='6bc56b87f051b8f74877f01f925fadc2e735b853'
mkdir -p -- "$repo_root/target"
test_root="$(mktemp -d "$repo_root/target/.contract-drift-test.XXXXXX")"
clone="$test_root/repo"
shim_dir="$test_root/bin"
tool_log="$test_root/forged-tool.log"
cleanup() { rm -rf -- "$test_root"; }
trap cleanup EXIT

git clone --quiet --no-local "$repo_root" "$clone"
mkdir -- "$shim_dir"
script="$clone/ops/ci/contract-drift.sh"
ci_lib="$clone/ops/ci/lib.sh"
receipt_relative='target/contract-drift/receipt.json'
receipt="$clone/$receipt_relative"
evidence_root="$clone/target/contract-drift"

fail() {
  printf 'contract drift hostile test failed: %s\n' "$*" >&2
  exit 1
}

expect_failure() {
  local description="$1"
  shift
  if "$@" > "$test_root/stdout" 2> "$test_root/stderr"; then
    fail "$description was accepted"
  fi
}

expect_validation_failure() {
  local description="$1"
  expect_failure "$description" \
    bash -c "cd '$clone' && bash ops/ci/contract-drift.sh --validate-receipt '$receipt_relative'"
}

update_output_digest() {
  local package="$1" output="$2" digest tmp
  digest="$(sha256sum -- "$output" | awk '{print $1}')"
  tmp="$receipt.tmp"
  jq --arg package "$package" --arg digest "$digest" \
    '(.public_api[] | select(.package == $package) | .sha256) = $digest' \
    "$receipt" > "$tmp"
  mv -f -- "$tmp" "$receipt"
}

git -C "$clone" tag -f "$baseline_tag" HEAD >/dev/null
expect_failure 'wrong immutable baseline tag' \
  bash -c "cd '$clone' && bash ops/ci/contract-drift.sh"
git -C "$clone" tag -f "$baseline_tag" "$baseline_commit" >/dev/null

printf '\n# hostile drift\n' >> "$clone/config/jeryu-cache-policy.toml"
expect_failure 'dirty policy/config contract' \
  bash -c "cd '$clone' && bash ops/ci/contract-drift.sh"
git -C "$clone" restore config/jeryu-cache-policy.toml

# Seed the exact legacy v1 layout. Generation must migrate only these known
# derived entries and leave a closed five-file v2 evidence root.
mkdir -p -- "$evidence_root/cargo-target"
for legacy_package in jeryu-cache-core jeryu-cache-service \
  jeryu-cache-adversary jeryu-cache; do
  printf 'legacy\n' > \
    "$evidence_root/$legacy_package.public-api.diff.txt"
done

# Reproduce the rejected reviewer vector exactly: an exported Bash `cargo`
# function claims the governed version and returns success with empty reports.
# A forged PATH cargo-public-api symlink is present at the same time. Neither may
# execute; the fixed physical binary must produce the normalized evidence.
fake_tool="$test_root/forged-cargo-public-api"
cat > "$fake_tool" <<'SHIM'
#!/usr/bin/env bash
printf 'forged-path\n' >> "$HOSTILE_TOOL_LOG"
if [[ "${1:-}" == --version ]]; then
  printf 'cargo-public-api 0.52.0\n'
fi
exit 0
SHIM
chmod 0755 "$fake_tool"
ln -s -- "$fake_tool" "$shim_dir/cargo-public-api"
: > "$tool_log"
HOSTILE_REAL_CARGO="$(type -P cargo)"
HOSTILE_TOOL_LOG="$tool_log"
export HOSTILE_REAL_CARGO HOSTILE_TOOL_LOG
cargo() {
  if [[ "${1:-}" == public-api ]]; then
    printf 'forged-function\n' >> "$HOSTILE_TOOL_LOG"
    if [[ "${2:-}" == --version ]]; then
      printf 'cargo-public-api 0.52.0\n'
    fi
    return 0
  fi
  "$HOSTILE_REAL_CARGO" "$@"
}
export -f cargo
PATH="$shim_dir:$PATH" bash -c \
  "cd '$clone' && bash ops/ci/contract-drift.sh" \
  > "$test_root/stdout" 2> "$test_root/stderr" \
  || fail "governed direct invocation failed under forged cargo function/PATH: \
$(<"$test_root/stderr")"
unset -f cargo
[[ ! -s "$tool_log" ]] || fail 'forged cargo function or PATH tool executed'
if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
  expected_tool_path='/opt/jain-ci/native-build-tools/36801d2417bbd9a804e09f30a8fd3ca96a9b5eb5e27ef9820143d44f07f4e240/bin/cargo-public-api'
  expected_tool_custody='release-read-only-native-build-tools-v2'
else
  expected_tool_path='/var/lib/jain-host-ci/native-build-tools/36801d2417bbd9a804e09f30a8fd3ca96a9b5eb5e27ef9820143d44f07f4e240/bin/cargo-public-api'
  expected_tool_custody='local-root-owned-native-build-tools-v2'
fi
grep -Fq 'CARGO_TARGET_DIR="$contract_target" "$cargo_public_api_exec" -p "$package"' \
  "$script" || fail 'contract lane does not invoke the authenticated held FD'
grep -Fq 'jeryu_assert_closed_source_authority "$checkout" "$expected_head"' \
  "$script" || fail 'contract lane does not enforce closed physical source authority'
grep -Fq 'ls-files -v -z' "$ci_lib" \
  || fail 'shared source authority does not reject hidden index flags'
grep -Fq 'ls-files --others --ignored' "$ci_lib" \
  || fail 'shared source authority does not enumerate ignored inputs'
grep -Fq 'ls-tree -rz --full-tree' "$ci_lib" \
  || fail 'shared source authority is not derived from the exact HEAD tree'
grep -Fq 'hash-object --no-filters' "$ci_lib" \
  || fail 'shared source authority does not compare physical bytes to HEAD blobs'
grep -Fq '/usr/bin/env -i' "$ci_lib" \
  || fail 'governed Git invocation does not clear ambient authority'
grep -Fq '/usr/bin/git --no-replace-objects' "$ci_lib" \
  || fail 'governed Git invocation is not fixed and replacement-blind'
grep -Fq 'GIT_CONFIG_GLOBAL=/dev/null GIT_ATTR_NOSYSTEM=1' "$ci_lib" \
  || fail 'governed Git invocation retains global config or attributes authority'
grep -Fq -- "--format='%(refname)' refs/replace/" "$ci_lib" \
  || fail 'shared source authority does not reject canonical replacement refs'
grep -Fq 'jeryu_reject_git_replacement_authority "$repo_root"' "$script" \
  || fail 'contract entrypoint lacks the early replacement-ref gate'
grep -Fq "jeryu_reject_ambient_git_authority 'contract entrypoint" "$script" \
  || fail 'contract entrypoint lacks the closed Git-environment gate'
[[ "$(grep -Fc 'assert_cargo_public_api_stable' "$script")" -ge 4 ]] \
  || fail 'contract lane lacks pre/post held-FD identity checks'
grep -Fq 'find -H "$evidence_root_exec"' "$script" \
  || fail 'contract evidence enumeration is not rooted at the held directory FD'
grep -Fq 'legacy_target="$evidence_root_exec/cargo-target"' "$script" \
  || fail 'legacy evidence removal is not rooted at the held directory FD'
grep -Fq 'evidence_file_digest receipt.json' "$script" \
  || fail 'contract receipt reads are not rooted at the held directory FD'
grep -Fq 'exec {fd}<"$held"' "$script" \
  || fail 'contract subsidiaries are not opened relative to the held evidence root'
[[ "$(grep -Fc 'assert_exact_evidence_entries' "$script")" -ge 4 ]] \
  || fail 'contract evidence membership is not revalidated through completion'
custody_line="$(grep -n -m1 'actual_custody=.*stat' "$script" | cut -d: -f1)"
version_line="$(grep -n -m1 'actual_version=.*cargo_public_api_exec' "$script" | cut -d: -f1)"
[[ -n "$custody_line" && -n "$version_line" && "$custody_line" -lt "$version_line" ]] \
  || fail 'cargo-public-api custody is not enforced before first execution'
jq -e --arg tool_path "$expected_tool_path" \
  --arg tool_custody "$expected_tool_custody" '
    .schema_version == "jeryu.cache.contract-drift/v2" and
    .tool_path == $tool_path and
    .tool_custody == $tool_custody and
    .tool_sha256 == "a903554dd723f83cb8fafb370c42cfb27b8eac1cbbd9f93b89d6ceaa33714798" and
    [.public_api[].package] == ["jeryu-cache-core","jeryu-cache-service",
      "jeryu-cache-adversary","jeryu-cache"]
  ' "$receipt" >/dev/null || fail 'governed receipt identity/package set is not exact'
[[ "$(stat -Lc '%u:%g:%a:%h' -- "$expected_tool_path")" == '0:0:555:1' \
  && "$(stat -Lc '%u:%g:%a' -- "$(dirname -- "$expected_tool_path")")" \
    == '0:0:555' \
  && ( "${JAIN_RELEASE_CI:-0}" == 1 \
    || "$(stat -Lc '%u' -- "$expected_tool_path")" != "$EUID" ) \
  && ! -w "$expected_tool_path" ]] \
  || fail 'selected cargo-public-api remains caller-writable'

if [[ "${JAIN_RELEASE_CI:-0}" != 1 ]]; then
  mutable_bundle="$test_root/caller-owned-tools"
  mkdir -p -- "$mutable_bundle/bin"
  cp --reflink=auto -- "$expected_tool_path" "$mutable_bundle/bin/cargo-public-api"
  chmod 0755 "$mutable_bundle/bin"
  chmod 0555 "$mutable_bundle/bin/cargo-public-api"
  sed -i "s|^local_tool_root=.*|local_tool_root='$mutable_bundle'|" "$script"
  expect_failure 'caller-owned cargo-public-api custody' bash -c \
    "cd '$clone' && bash ops/ci/contract-drift.sh --validate-receipt '$receipt_relative'"
  grep -Fq 'not root-owned mode 0555 non-writable custody' "$test_root/stderr" \
    || fail 'caller-owned cargo-public-api did not fail at the pre-execution custody gate'
  git -C "$clone" restore ops/ci/contract-drift.sh
fi

# Same-inode mutation is separately observable by the governed digest fence.
# Production never uses this writable copy: both real custody domains above are
# non-writable (local mode0555 root ownership; release read-only mount).
mutable_tool="$test_root/mutable-cargo-public-api"
cp --reflink=auto -- "$expected_tool_path" "$mutable_tool"
chmod 0755 "$mutable_tool"
exec {mutable_fd}<"$mutable_tool"
mutable_inode_before="$(stat -Lc '%d:%i' -- "/proc/self/fd/$mutable_fd")"
mutable_sha_before="$(sha256sum -- "/proc/self/fd/$mutable_fd" | awk '{print $1}')"
printf 'hostile-in-place-write\n' >> "$mutable_tool"
mutable_inode_after="$(stat -Lc '%d:%i' -- "/proc/self/fd/$mutable_fd")"
mutable_sha_after="$(sha256sum -- "/proc/self/fd/$mutable_fd" | awk '{print $1}')"
exec {mutable_fd}<&-
[[ "$mutable_inode_before" == "$mutable_inode_after" \
  && "$mutable_sha_before" == 'a903554dd723f83cb8fafb370c42cfb27b8eac1cbbd9f93b89d6ceaa33714798' \
  && "$mutable_sha_after" != "$mutable_sha_before" ]] \
  || fail 'same-inode cargo-public-api mutation was not detected by digest'
while IFS= read -r output; do
  [[ -s "$clone/$output" ]] || fail "normalized evidence is empty: $output"
done < <(jq -r '.public_api[].output' "$receipt")
[[ "$(find "$evidence_root" -mindepth 1 -maxdepth 1 -type f | wc -l | tr -d ' ')" \
    == 5 \
  && "$(find "$evidence_root" -mindepth 1 -maxdepth 1 ! -type f | wc -l | tr -d ' ')" \
    == 0 ]] || fail 'legacy contract evidence did not migrate to the exact five-file root'

# A path swap after authentication cannot redirect the held descriptor. The
# forged canonical inode is observably different, /proc/self/fd still executes
# governed bytes, and restoring the name never executes the forgery.
held_tool="$test_root/held-cargo-public-api"
held_name="$held_tool.governed"
cp --reflink=auto -- "$expected_tool_path" "$held_tool"
chmod 0755 "$held_tool"
exec {held_fd}<"$held_tool"
held_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- \
  "/proc/self/fd/$held_fd")"
mv -- "$held_tool" "$held_name"
cp -- "$fake_tool" "$held_tool"
chmod 0755 "$held_tool"
swapped_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$held_tool")"
[[ "$swapped_identity" != "$held_identity" ]] \
  || fail 'swap/restore hostile did not replace the canonical inode'
held_version="$("/proc/self/fd/$held_fd" --version)"
held_sha="$(sha256sum -- "/proc/self/fd/$held_fd" | awk '{print $1}')"
rm -- "$held_tool"
mv -- "$held_name" "$held_tool"
restored_inode="$(stat -Lc '%d:%i' -- "$held_tool")"
fd_inode="$(stat -Lc '%d:%i' -- "/proc/self/fd/$held_fd")"
exec {held_fd}<&-
[[ "$held_version" == 'cargo-public-api 0.52.0' \
  && "$held_sha" == 'a903554dd723f83cb8fafb370c42cfb27b8eac1cbbd9f93b89d6ceaa33714798' \
  && "$restored_inode" == "$fd_inode" && ! -s "$tool_log" ]] \
  || fail 'held-FD cargo-public-api swap/restore executed or accepted forged bytes'

# A same-name PATH hardlink is also ignored; validation remains bound to the
# fixed governed path and its single-link physical identity.
rm -- "$shim_dir/cargo-public-api"
ln -- "$fake_tool" "$shim_dir/cargo-public-api"
PATH="$shim_dir:$PATH" bash -c \
  "cd '$clone' && bash ops/ci/contract-drift.sh --validate-receipt '$receipt_relative'" \
  >/dev/null || fail 'PATH hardlink changed governed tool validation'
[[ ! -s "$tool_log" ]] || fail 'forged PATH hardlink executed'

pristine="$test_root/pristine-contract-evidence"
cp -a -- "$evidence_root" "$pristine"
reset_evidence() {
  if [[ -L "$evidence_root" ]]; then
    rm -- "$evidence_root"
  elif [[ -e "$evidence_root" ]]; then
    rm -rf -- "$evidence_root"
  fi
  cp -a -- "$pristine" "$evidence_root"
}

clone_head="$(git -C "$clone" rev-parse 'HEAD^{commit}')"
clone_tree="$(git -C "$clone" rev-parse 'HEAD^{tree}')"

assert_authority_restored() {
  local description="$1" dirty forbidden_flags replacement_refs
  [[ "$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
      rev-parse 'HEAD^{commit}')" == "$clone_head" \
    && "$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
      rev-parse 'HEAD^{tree}')" == "$clone_tree" ]] \
    || fail "$description did not restore the exact head/tree"
  dirty="$(git -C "$clone" status --porcelain=v1 --untracked-files=all)"
  [[ -z "$dirty" ]] || fail "$description left checkout porcelain"
  forbidden_flags="$(git -C "$clone" ls-files -v | LC_ALL=C grep -v '^H ' || true)"
  [[ -z "$forbidden_flags" ]] || fail "$description left forbidden index flags"
  replacement_refs="$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects \
    -C "$clone" for-each-ref --format='%(refname)' \
    refs/replace/ refs/hostile-replace/)"
  [[ -z "$replacement_refs" ]] || fail "$description left replacement refs"
}

expect_authority_rejection() {
  local description="$1" global_config="${2:-}" receipt_before receipt_after
  local -a environment=()
  [[ -z "$global_config" ]] || environment=(env "GIT_CONFIG_GLOBAL=$global_config")
  receipt_before="$(sha256sum -- "$receipt" | awk '{print $1}')"
  expect_failure "$description generation" "${environment[@]}" bash -c \
    "cd '$clone' && bash ops/ci/contract-drift.sh"
  receipt_after="$(sha256sum -- "$receipt" | awk '{print $1}')"
  [[ "$receipt_after" == "$receipt_before" ]] \
    || fail "$description generation replaced the prior receipt"
  expect_failure "$description validation" "${environment[@]}" bash -c \
    "cd '$clone' && bash ops/ci/contract-drift.sh --validate-receipt '$receipt_relative'"
  receipt_after="$(sha256sum -- "$receipt" | awk '{print $1}')"
  [[ "$receipt_after" == "$receipt_before" ]] \
    || fail "$description validation replaced the prior receipt"
}

expect_git_authority_rejection() {
  local description="$1" marker="$2" receipt_before receipt_after
  shift 2
  local -a environment=("$@")
  receipt_before="$(sha256sum -- "$receipt" | awk '{print $1}')"
  expect_failure "$description generation" "${environment[@]}" bash -c \
    "cd '$clone' && bash ops/ci/contract-drift.sh"
  grep -Fq "$marker" "$test_root/stderr" \
    || fail "$description generation missed the explicit replacement gate"
  receipt_after="$(sha256sum -- "$receipt" | awk '{print $1}')"
  [[ "$receipt_after" == "$receipt_before" ]] \
    || fail "$description generation replaced the prior receipt"
  expect_failure "$description validation" "${environment[@]}" bash -c \
    "cd '$clone' && bash ops/ci/contract-drift.sh --validate-receipt '$receipt_relative'"
  grep -Fq "$marker" "$test_root/stderr" \
    || fail "$description validation missed the explicit replacement gate"
  receipt_after="$(sha256sum -- "$receipt" | awk '{print $1}')"
  [[ "$receipt_after" == "$receipt_before" ]] \
    || fail "$description validation replaced the prior receipt"
}

# Build a replacement commit that preserves the successor proof scripts while
# substituting one governed build input. This lets both real entrypoints execute
# their new early gate while ordinary Git reports a clean substituted tree.
printf '\n# replacement-object hostile\n' >> "$clone/.cargo/config.toml"
git -C "$clone" add -- .cargo/config.toml
git -C "$clone" -c user.name='Hostile Test' -c user.email='hostile@example.invalid' \
  commit -m 'hostile replacement tree' >/dev/null
replacement_commit="$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects \
  -C "$clone" rev-parse 'HEAD^{commit}')"
replacement_tree="$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects \
  -C "$clone" rev-parse 'HEAD^{tree}')"
[[ "$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
    rev-parse "$replacement_commit:ops/ci/contract-drift.sh")" \
  == "$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
    rev-parse "$clone_head:ops/ci/contract-drift.sh")" ]] \
  || fail 'replacement fixture did not preserve the successor contract entrypoint'
GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
  reset --hard "$clone_head" >/dev/null
assert_authority_restored 'replacement fixture preparation'

git -C "$clone" replace "$clone_head" "$replacement_commit"
git -C "$clone" reset --hard HEAD >/dev/null
[[ "$(git -C "$clone" rev-parse 'HEAD^{commit}')" == "$clone_head" \
  && "$(git -C "$clone" rev-parse 'HEAD^{tree}')" == "$replacement_tree" \
  && -z "$(git -C "$clone" status --porcelain=v1 --untracked-files=all)" ]] \
  || fail 'canonical replacement hostile did not reproduce clean-tree substitution'
expect_git_authority_rejection 'canonical replacement-ref substitution' \
  'rejects canonical Git replacement refs'
GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
  update-ref -d "refs/replace/$clone_head"
GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
  reset --hard "$clone_head" >/dev/null
assert_authority_restored 'canonical replacement hostile'

alternate_replace_base='refs/hostile-replace'
GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" update-ref \
  "$alternate_replace_base/$clone_head" "$replacement_commit"
GIT_REPLACE_REF_BASE="$alternate_replace_base" git -C "$clone" reset --hard HEAD \
  >/dev/null
[[ "$(GIT_REPLACE_REF_BASE="$alternate_replace_base" git -C "$clone" \
      rev-parse 'HEAD^{commit}')" == "$clone_head" \
  && "$(GIT_REPLACE_REF_BASE="$alternate_replace_base" git -C "$clone" \
      rev-parse 'HEAD^{tree}')" == "$replacement_tree" \
  && -z "$(GIT_REPLACE_REF_BASE="$alternate_replace_base" git -C "$clone" \
      status --porcelain=v1 --untracked-files=all)" ]] \
  || fail 'alternate replacement namespace did not reproduce clean-tree substitution'
expect_git_authority_rejection 'alternate replacement namespace substitution' \
  'rejects ambient Git authority variable: GIT_REPLACE_REF_BASE' \
  env "GIT_REPLACE_REF_BASE=$alternate_replace_base"
GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
  reset --hard "$clone_head" >/dev/null
GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" update-ref \
  -d "$alternate_replace_base/$clone_head"
assert_authority_restored 'alternate replacement namespace hostile'

# Load the already-authenticated successor helper so its fixed Git wrapper can
# be tested independently from the entrypoint's ambient-environment rejection.
# shellcheck source=/dev/null
source "$ci_lib"

# The required gate authenticates Jankurai first, which deliberately exports
# this exact fail-closed value. It must compose with the source-authority guard,
# while any other caller-selected value remains rejected.
GIT_TERMINAL_PROMPT=0 jeryu_reject_ambient_git_authority \
  'fixed noninteractive Git contract' \
  || fail 'fixed GIT_TERMINAL_PROMPT=0 was rejected'
GIT_TERMINAL_PROMPT=0 bash -c \
  "cd '$clone' && bash ops/ci/contract-drift.sh --validate-receipt '$receipt_relative'" \
  >/dev/null || fail 'contract receipt rejected the fixed noninteractive Git value'
expect_git_authority_rejection 'nonzero GIT_TERMINAL_PROMPT' \
  'rejects ambient Git authority variable: GIT_TERMINAL_PROMPT' \
  env GIT_TERMINAL_PROMPT=1

# Reproduce the foreign Git-authority forgery with a distinct commit carrying
# the exact victim tree. Ordinary Git follows the foreign .git directory while
# the physical victim HEAD remains unchanged; the entrypoint rejects the env.
foreign_repo="$test_root/foreign-authority"
git clone --quiet --no-local "$clone" "$foreign_repo"
git -C "$foreign_repo" -c user.name='Hostile Test' \
  -c user.email='hostile@example.invalid' commit --allow-empty \
  -m 'foreign same-tree authority' >/dev/null
foreign_head="$(git -C "$foreign_repo" rev-parse 'HEAD^{commit}')"
foreign_tree="$(git -C "$foreign_repo" rev-parse 'HEAD^{tree}')"
foreign_git_dir="$(git -C "$foreign_repo" rev-parse --absolute-git-dir)"
victim_git_dir="$(git -C "$clone" rev-parse --absolute-git-dir)"
[[ "$foreign_head" != "$clone_head" && "$foreign_tree" == "$clone_tree" \
  && "$(GIT_DIR="$foreign_git_dir" GIT_WORK_TREE="$clone" git -C "$clone" \
      rev-parse 'HEAD^{commit}')" == "$foreign_head" \
  && "$(GIT_DIR="$foreign_git_dir" GIT_WORK_TREE="$clone" \
      jeryu_governed_git -C "$clone" rev-parse 'HEAD^{commit}')" == "$clone_head" \
  && "$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
      rev-parse 'HEAD^{commit}')" == "$clone_head" ]] \
  || fail 'foreign same-tree Git authority forgery was not reproduced'
expect_git_authority_rejection 'foreign same-tree Git authority' \
  'rejects ambient Git authority variable:' env \
  "GIT_DIR=$foreign_git_dir" "GIT_WORK_TREE=$clone"

for authority_env in \
  "GIT_DIR=$foreign_git_dir" \
  "GIT_WORK_TREE=$clone" \
  "GIT_COMMON_DIR=$foreign_git_dir" \
  "GIT_INDEX_FILE=$victim_git_dir/index" \
  "GIT_OBJECT_DIRECTORY=$victim_git_dir/objects" \
  "GIT_ALTERNATE_OBJECT_DIRECTORIES=$foreign_git_dir/objects"; do
  authority_name="${authority_env%%=*}"
  expect_git_authority_rejection "ambient $authority_name" \
    "rejects ambient Git authority variable: $authority_name" \
    env "$authority_env"
done
expect_git_authority_rejection 'ambient GIT_CONFIG_COUNT tuple' \
  'rejects ambient Git authority variable:' env \
  GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.fileMode GIT_CONFIG_VALUE_0=false
assert_authority_restored 'ambient Git authority matrix'

# Self-reference limit: a full replace+reset can replace this entrypoint with
# its predecessor bytes. The already-loaded successor guard rejects the ref,
# but the physical predecessor entrypoint is deliberately not treated as proof;
# only the external root-owned exact materializer can authenticate those bytes.
clone_parent="$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
  rev-parse "$clone_head^")"
receipt_before="$(sha256sum -- "$receipt" | awk '{print $1}')"
git -C "$clone" replace "$clone_head" "$clone_parent"
git -C "$clone" reset --hard HEAD >/dev/null
[[ "$(git -C "$clone" rev-parse 'HEAD^{commit}')" == "$clone_head" \
  && "$(git -C "$clone" rev-parse 'HEAD^{tree}')" \
    == "$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
      rev-parse "$clone_parent^{tree}")" \
  && -z "$(git -C "$clone" status --porcelain=v1 --untracked-files=all)" \
  && "$(git -C "$clone" hash-object --no-filters -- ops/ci/contract-drift.sh)" \
    == "$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
      rev-parse "$clone_parent:ops/ci/contract-drift.sh")" ]] \
  || fail 'full predecessor replacement did not reproduce entrypoint substitution'
if jeryu_reject_git_replacement_authority "$clone" \
  'preloaded successor contract guard' > "$test_root/stdout" 2> "$test_root/stderr"; then
  fail 'preloaded successor guard accepted a full predecessor replacement'
fi
grep -Fq 'rejects canonical Git replacement refs' "$test_root/stderr" \
  || fail 'preloaded successor guard missed full predecessor replacement'
[[ "$(sha256sum -- "$receipt" | awk '{print $1}')" == "$receipt_before" ]] \
  || fail 'entrypoint substitution probe changed the prior receipt'
GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
  update-ref -d "refs/replace/$clone_head"
GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
  reset --hard "$clone_head" >/dev/null
assert_authority_restored 'full predecessor self-reference probe'

# Repository-local excludes must not turn a source/build input into invisible
# authority. Both generation and validation must fail before touching evidence.
info_exclude="$(git -C "$clone" rev-parse --absolute-git-dir)/info/exclude"
cp -- "$info_exclude" "$test_root/info-exclude.saved"
printf '/.cargo/config\n' >> "$info_exclude"
printf '[build]\ntarget-dir = "target/hostile"\n' > "$clone/.cargo/config"
[[ -z "$(git -C "$clone" status --porcelain=v1 --untracked-files=all)" ]] \
  || fail 'info/exclude hostile was not hidden from porcelain'
git -C "$clone" check-ignore -q -- .cargo/config \
  || fail 'info/exclude hostile was not ignored'
expect_authority_rejection 'info/exclude hidden .cargo input'
rm -- "$clone/.cargo/config"
mv -f -- "$test_root/info-exclude.saved" "$info_exclude"
assert_authority_restored 'info/exclude hostile'

# A caller-selected global excludes file is part of standard Git exclusion
# semantics and receives the same fail-closed treatment.
global_excludes="$test_root/global-excludes"
global_config="$test_root/global.gitconfig"
printf '/.cargo/config\n' > "$global_excludes"
git config --file "$global_config" core.excludesFile "$global_excludes"
printf '[build]\ntarget-dir = "target/hostile"\n' > "$clone/.cargo/config"
[[ -z "$(GIT_CONFIG_GLOBAL="$global_config" git -C "$clone" \
  status --porcelain=v1 --untracked-files=all)" ]] \
  || fail 'global excludes hostile was not hidden from porcelain'
GIT_CONFIG_GLOBAL="$global_config" git -C "$clone" check-ignore -q -- .cargo/config \
  || fail 'global excludes hostile was not ignored'
expect_authority_rejection 'global excludes hidden .cargo input' "$global_config"
rm -- "$clone/.cargo/config"
assert_authority_restored 'global excludes hostile'

# Index promises cannot replace a physical comparison to the governed HEAD.
git -C "$clone" update-index --assume-unchanged .cargo/config.toml
printf '\n# assume-unchanged hostile\n' >> "$clone/.cargo/config.toml"
[[ -z "$(git -C "$clone" status --porcelain=v1 --untracked-files=all)" \
  && "$(git -C "$clone" ls-files -v -- .cargo/config.toml)" == h\ * \
  && "$(git -C "$clone" rev-parse 'HEAD:.cargo/config.toml')" \
    != "$(git -C "$clone" hash-object --no-filters -- .cargo/config.toml)" ]] \
  || fail 'assume-unchanged hostile did not reproduce the porcelain evasion'
expect_authority_rejection 'assume-unchanged modified tracked input'
git -C "$clone" update-index --no-assume-unchanged .cargo/config.toml
git -C "$clone" restore -- .cargo/config.toml
assert_authority_restored 'assume-unchanged hostile'

git -C "$clone" update-index --skip-worktree rust-toolchain.toml
printf '\n# skip-worktree hostile\n' >> "$clone/rust-toolchain.toml"
[[ -z "$(git -C "$clone" status --porcelain=v1 --untracked-files=all)" \
  && "$(git -C "$clone" ls-files -v -- rust-toolchain.toml)" == [Ss]\ * \
  && "$(git -C "$clone" rev-parse 'HEAD:rust-toolchain.toml')" \
    != "$(git -C "$clone" hash-object --no-filters -- rust-toolchain.toml)" ]] \
  || fail 'skip-worktree hostile did not reproduce the porcelain evasion'
expect_authority_rejection 'skip-worktree modified tracked input'
git -C "$clone" update-index --no-skip-worktree rust-toolchain.toml
git -C "$clone" restore -- rust-toolchain.toml
assert_authority_restored 'skip-worktree hostile'

# Content-identical hard links, symlinks, symlinked ancestors, and executable
# mode mismatches are all outside the physical HEAD materialization contract.
tracked_input="$clone/.cargo/config.toml"
mv -- "$tracked_input" "$test_root/config.toml.physical"
ln -- "$test_root/config.toml.physical" "$tracked_input"
[[ "$(stat -Lc '%h' -- "$tracked_input")" == 2 ]] \
  || fail 'tracked hardlink hostile did not create two names'
expect_authority_rejection 'hard-linked tracked input'
rm -- "$tracked_input"
mv -- "$test_root/config.toml.physical" "$tracked_input"
assert_authority_restored 'tracked hardlink hostile'

mv -- "$tracked_input" "$test_root/config.toml.physical"
ln -s -- "$test_root/config.toml.physical" "$tracked_input"
expect_authority_rejection 'symlinked tracked input'
rm -- "$tracked_input"
mv -- "$test_root/config.toml.physical" "$tracked_input"
assert_authority_restored 'tracked symlink hostile'

mv -- "$clone/.cargo" "$test_root/cargo.physical"
ln -s -- "$test_root/cargo.physical" "$clone/.cargo"
expect_authority_rejection 'symlinked tracked input parent'
rm -- "$clone/.cargo"
mv -- "$test_root/cargo.physical" "$clone/.cargo"
assert_authority_restored 'tracked parent symlink hostile'

original_filemode="$(git -C "$clone" config --local --get core.fileMode || true)"
git -C "$clone" config --local core.fileMode false
chmod 0755 -- "$tracked_input"
[[ -z "$(git -C "$clone" status --porcelain=v1 --untracked-files=all)" ]] \
  || fail 'executable-bit hostile was not hidden from porcelain'
expect_authority_rejection 'tracked executable-bit mismatch'
chmod 0644 -- "$tracked_input"
assert_authority_restored 'tracked executable-bit hostile'
chmod 0666 -- "$tracked_input"
[[ -z "$(git -C "$clone" status --porcelain=v1 --untracked-files=all)" ]] \
  || fail 'writable-mode hostile was not hidden from porcelain'
expect_authority_rejection 'tracked group/world-writable mode'
chmod 0644 -- "$tracked_input"
if [[ -n "$original_filemode" ]]; then
  git -C "$clone" config --local core.fileMode "$original_filemode"
else
  git -C "$clone" config --local --unset core.fileMode
fi
assert_authority_restored 'tracked writable-mode hostile'

package='jeryu-cache-core'
output="$evidence_root/$package.public-api.diff.json"

reset_evidence
: > "$output"
update_output_digest "$package" "$output"
expect_validation_failure 'digest-bound empty normalized report'

reset_evidence
printf '{}\n' > "$output"
update_output_digest "$package" "$output"
expect_validation_failure 'digest-bound unstructured normalized report'

reset_evidence
tmp="$output.tmp"
jq '.raw_report = "" | .raw_report_bytes = 0 |
  .raw_report_sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"' \
  "$output" > "$tmp"
mv -f -- "$tmp" "$output"
update_output_digest "$package" "$output"
expect_validation_failure 'structured wrapper containing an empty raw report'

reset_evidence
tmp="$receipt.tmp"
jq '.public_api[3].package = "jeryu-cache-core"' "$receipt" > "$tmp"
mv -f -- "$tmp" "$receipt"
expect_validation_failure 'duplicate/forged four-package set'

reset_evidence
printf 'unexpected\n' > "$evidence_root/unexpected.txt"
expect_validation_failure 'unknown contract evidence subsidiary'

reset_evidence
mv -- "$output" "$output.physical"
ln -s -- "$(basename "$output.physical")" "$output"
expect_validation_failure 'symlinked normalized evidence'

reset_evidence
mv -- "$output" "$output.physical"
ln -- "$output.physical" "$output"
expect_validation_failure 'hard-linked normalized evidence'

reset_evidence
mv -- "$receipt" "$receipt.physical"
ln -s -- "$(basename "$receipt.physical")" "$receipt"
expect_validation_failure 'symlinked contract receipt'

reset_evidence
mv -- "$receipt" "$receipt.physical"
ln -- "$receipt.physical" "$receipt"
expect_validation_failure 'hard-linked contract receipt'

reset_evidence
physical_root="$clone/target/contract-drift.physical"
mv -- "$evidence_root" "$physical_root"
ln -s -- "$(basename "$physical_root")" "$evidence_root"
expect_validation_failure 'symlinked contract evidence parent'
rm -- "$evidence_root"
mv -- "$physical_root" "$evidence_root"

cp -- "$receipt" "$test_root/noncanonical-receipt.json"
expect_failure 'noncanonical receipt path' bash -c \
  "cd '$clone' && bash ops/ci/contract-drift.sh --validate-receipt '$test_root/noncanonical-receipt.json'"

printf '\n# hostile build configuration\n' >> "$clone/.cargo/config.toml"
expect_validation_failure 'dirty .cargo build configuration'
git -C "$clone" restore .cargo/config.toml

printf '\n# hostile toolchain\n' >> "$clone/rust-toolchain.toml"
expect_validation_failure 'dirty rust-toolchain.toml'
git -C "$clone" restore rust-toolchain.toml

git -C "$clone" -c user.name='Hostile Test' -c user.email='hostile@example.invalid' \
  commit --allow-empty -m 'hostile moving head' >/dev/null
expect_validation_failure 'moved source head after evidence generation'

printf 'contract drift hostile tests ok\n'
