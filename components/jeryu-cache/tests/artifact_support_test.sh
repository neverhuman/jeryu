#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p -- "$repo_root/target"
test_root="$(mktemp -d "$repo_root/target/.artifact-support-test.XXXXXX")"
clone="$test_root/repo"
shim_dir="$test_root/bin"
tool_log="$test_root/forged-syft.log"
cleanup() { rm -rf -- "$test_root"; }
trap cleanup EXIT

git clone --quiet --no-local "$repo_root" "$clone"
mkdir -- "$shim_dir"
script="$clone/ops/ci/artifact_support.sh"
ci_lib="$clone/ops/ci/lib.sh"
receipt_relative='target/artifact-support/jeryu-cache.json'
receipt="$clone/$receipt_relative"
evidence_root="$clone/target/artifact-support"
inventory="$evidence_root/jeryu-cache-release-inputs.sha256"

fail() {
  printf 'artifact support hostile test failed: %s\n' "$*" >&2
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
  expect_failure "$description" bash -c \
    "cd '$clone' && bash ops/ci/artifact_support.sh --validate-receipt '$receipt_relative'"
}

# A caller-controlled PATH Syft symlink and Bash function must never execute.
# The lane directly invokes the fixed physical binary and binds its SHA/custody.
fake_tool="$test_root/forged-syft"
cat > "$fake_tool" <<'SHIM'
#!/usr/bin/env bash
printf 'forged-path\n' >> "$HOSTILE_TOOL_LOG"
printf '{"application":"syft","version":"1.40.0"}\n'
exit 0
SHIM
chmod 0755 "$fake_tool"
ln -s -- "$fake_tool" "$shim_dir/syft"
: > "$tool_log"
HOSTILE_TOOL_LOG="$tool_log"
export HOSTILE_TOOL_LOG
syft() {
  printf 'forged-function\n' >> "$HOSTILE_TOOL_LOG"
  printf '{"application":"syft","version":"1.40.0"}\n'
  return 0
}
export -f syft
PATH="$shim_dir:$PATH" bash -c \
  "cd '$clone' && bash ops/ci/artifact_support.sh" \
  > "$test_root/stdout" 2> "$test_root/stderr" \
  || fail 'physical Syft artifact generation failed under forged function/PATH'
unset -f syft
[[ ! -s "$tool_log" ]] || fail 'forged Syft function or PATH tool executed'

if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
  expected_syft_path='/opt/jain-ci/authority/security-bin/syft'
  expected_syft_custody_domain='release-read-only-security-bind'
  expected_syft_custody="$(stat -Lc '%u:%g:%a:%h' -- "$expected_syft_path")"
else
  expected_syft_path='/var/lib/jain-host-ci/native-build-tools/36801d2417bbd9a804e09f30a8fd3ca96a9b5eb5e27ef9820143d44f07f4e240/bin/syft'
  expected_syft_custody_domain='local-root-owned-native-build-tools-v2'
  expected_syft_custody="$(stat -Lc '%u:%g:%a:%h' -- "$expected_syft_path")"
fi
grep -Fq 'SYFT_CHECK_FOR_APP_UPDATE=false "$syft_exec" dir:.' "$script" \
  || fail 'artifact lane does not invoke the authenticated held Syft FD'
grep -Fq 'jeryu_assert_closed_source_authority "$repo_root" "$expected_head"' \
  "$script" || fail 'artifact lane does not enforce closed physical source authority'
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
  || fail 'artifact entrypoint lacks the early replacement-ref gate'
grep -Fq "jeryu_reject_ambient_git_authority 'artifact entrypoint" "$script" \
  || fail 'artifact entrypoint lacks the closed Git-environment gate'
grep -Fq -- "--exclude './.jankurai/**'" "$script" \
  || fail 'Syft scan does not exclude allowed derived Jankurai state'
grep -Fq -- "--exclude './agent/repo-score.json'" "$script" \
  || fail 'Syft scan does not exclude the allowed derived score JSON'
grep -Fq -- "--exclude './agent/repo-score.md'" "$script" \
  || fail 'Syft scan does not exclude the allowed derived score Markdown'
[[ "$(grep -Fc 'assert_syft_stable' "$script")" -ge 4 ]] \
  || fail 'artifact lane lacks pre/post held-FD identity checks'
grep -Fq 'find -H "$evidence_root_exec"' "$script" \
  || fail 'artifact evidence enumeration is not rooted at the held directory FD'
grep -Fq '"$evidence_root_exec/jeryu-cache.json"' "$script" \
  || fail 'artifact evidence publication is not rooted at the held directory FD'
grep -Fq 'evidence_file_digest jeryu-cache.json' "$script" \
  || fail 'artifact receipt reads are not rooted at the held directory FD'
grep -Fq 'exec {fd}<"$held"' "$script" \
  || fail 'artifact subsidiaries are not opened relative to the held evidence root'
[[ "$(grep -Fc 'assert_exact_evidence_entries' "$script")" -ge 4 ]] \
  || fail 'artifact evidence membership is not revalidated through completion'
custody_line="$(grep -n -m1 'actual_custody=.*stat' "$script" | cut -d: -f1)"
version_line="$(grep -n -m1 'actual_version=.*syft_exec' "$script" | cut -d: -f1)"
[[ -n "$custody_line" && -n "$version_line" && "$custody_line" -lt "$version_line" ]] \
  || fail 'Syft custody is not enforced before first execution'
jq -e \
  --arg path "$expected_syft_path" \
  --arg custody "$expected_syft_custody_domain" \
  --arg uid "${expected_syft_custody%%:*}" \
  --arg rest "${expected_syft_custody#*:}" '
    .schema_version == "jeryu.split.artifact-support/v3" and
    .syft_path == $path and
    .syft_custody == $custody and
    .syft_sha256 == "eb9714fb8e4b8f2a647e7bb312f1e0b9f83a7aa30418658bf46583cfa83d27d2" and
    .syft_uid == $uid and
    ((.syft_gid + ":" + .syft_mode + ":" + .syft_nlink) == $rest)
  ' "$receipt" >/dev/null || fail 'receipt does not bind governed Syft custody'
[[ "$expected_syft_custody" == '0:0:555:1' \
  && "$(stat -Lc '%u:%g:%a' -- "$(dirname -- "$expected_syft_path")")" \
    == '0:0:555' \
  && ( "${JAIN_RELEASE_CI:-0}" == 1 \
    || "$(stat -Lc '%u' -- "$expected_syft_path")" != "$EUID" ) \
  && ! -w "$expected_syft_path" ]] \
  || fail 'selected Syft remains caller-writable'

if [[ "${JAIN_RELEASE_CI:-0}" != 1 ]]; then
  mutable_bundle="$test_root/caller-owned-tools"
  mkdir -p -- "$mutable_bundle/bin"
  cp --reflink=auto -- "$expected_syft_path" "$mutable_bundle/bin/syft"
  chmod 0755 "$mutable_bundle/bin"
  chmod 0555 "$mutable_bundle/bin/syft"
  sed -i "s|^local_tool_root=.*|local_tool_root='$mutable_bundle'|" "$script"
  expect_failure 'caller-owned Syft custody' bash -c \
    "cd '$clone' && bash ops/ci/artifact_support.sh --validate-receipt '$receipt_relative'"
  grep -Fq 'not root-owned mode 0555 non-writable custody' "$test_root/stderr" \
    || fail 'caller-owned Syft did not fail at the pre-execution custody gate'
  git -C "$clone" restore ops/ci/artifact_support.sh
fi

mutable_syft="$test_root/mutable-syft"
cp --reflink=auto -- "$expected_syft_path" "$mutable_syft"
chmod 0755 "$mutable_syft"
exec {mutable_fd}<"$mutable_syft"
mutable_inode_before="$(stat -Lc '%d:%i' -- "/proc/self/fd/$mutable_fd")"
mutable_sha_before="$(sha256sum -- "/proc/self/fd/$mutable_fd" | awk '{print $1}')"
printf 'hostile-in-place-write\n' >> "$mutable_syft"
mutable_inode_after="$(stat -Lc '%d:%i' -- "/proc/self/fd/$mutable_fd")"
mutable_sha_after="$(sha256sum -- "/proc/self/fd/$mutable_fd" | awk '{print $1}')"
exec {mutable_fd}<&-
[[ "$mutable_inode_before" == "$mutable_inode_after" \
  && "$mutable_sha_before" == 'eb9714fb8e4b8f2a647e7bb312f1e0b9f83a7aa30418658bf46583cfa83d27d2' \
  && "$mutable_sha_after" != "$mutable_sha_before" ]] \
  || fail 'same-inode Syft mutation was not detected by digest'
grep -Eq '^[0-9a-f]{64}  \.cargo/' "$inventory" \
  || fail 'generated inventory omitted .cargo configuration'
grep -Eq '^[0-9a-f]{64}  rust-toolchain\.toml$' "$inventory" \
  || fail 'generated inventory omitted rust-toolchain.toml'

# Exercise the exact held-FD swap/restore primitive without ever mutating the
# installed governed Syft path. A forged canonical inode is detectable while
# the descriptor continues to execute only the authenticated bytes.
held_syft="$test_root/held-syft"
held_name="$held_syft.governed"
cp --reflink=auto -- "$expected_syft_path" "$held_syft"
chmod 0755 "$held_syft"
exec {held_fd}<"$held_syft"
held_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- \
  "/proc/self/fd/$held_fd")"
mv -- "$held_syft" "$held_name"
cp -- "$fake_tool" "$held_syft"
chmod 0755 "$held_syft"
swapped_identity="$(stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "$held_syft")"
[[ "$swapped_identity" != "$held_identity" ]] \
  || fail 'Syft swap/restore hostile did not replace the canonical inode'
held_version="$("/proc/self/fd/$held_fd" version -o json | jq -er \
  'select(.application == "syft") | .version')"
held_sha="$(sha256sum -- "/proc/self/fd/$held_fd" | awk '{print $1}')"
rm -- "$held_syft"
mv -- "$held_name" "$held_syft"
restored_inode="$(stat -Lc '%d:%i' -- "$held_syft")"
fd_inode="$(stat -Lc '%d:%i' -- "/proc/self/fd/$held_fd")"
exec {held_fd}<&-
[[ "$held_version" == '1.40.0' \
  && "$held_sha" == 'eb9714fb8e4b8f2a647e7bb312f1e0b9f83a7aa30418658bf46583cfa83d27d2' \
  && "$restored_inode" == "$fd_inode" && ! -s "$tool_log" ]] \
  || fail 'held-FD Syft swap/restore executed or accepted forged bytes'

# A same-name PATH hardlink is also ignored during validation.
rm -- "$shim_dir/syft"
ln -- "$fake_tool" "$shim_dir/syft"
PATH="$shim_dir:$PATH" bash -c \
  "cd '$clone' && bash ops/ci/artifact_support.sh --validate-receipt '$receipt_relative'" \
  >/dev/null || fail 'PATH hardlink changed governed Syft validation'
[[ ! -s "$tool_log" ]] || fail 'forged PATH Syft hardlink executed'

[[ "$(find "$evidence_root" -mindepth 1 -maxdepth 1 -type f | wc -l | tr -d ' ')" \
    == 4 \
  && "$(find "$evidence_root" -mindepth 1 -maxdepth 1 ! -type f | wc -l | tr -d ' ')" \
    == 0 ]] || fail 'artifact evidence root is not the exact four-file set'

pristine="$test_root/pristine-artifact-evidence"
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
    "cd '$clone' && bash ops/ci/artifact_support.sh"
  receipt_after="$(sha256sum -- "$receipt" | awk '{print $1}')"
  [[ "$receipt_after" == "$receipt_before" ]] \
    || fail "$description generation replaced the prior receipt"
  expect_failure "$description validation" "${environment[@]}" bash -c \
    "cd '$clone' && bash ops/ci/artifact_support.sh --validate-receipt '$receipt_relative'"
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
    "cd '$clone' && bash ops/ci/artifact_support.sh"
  grep -Fq "$marker" "$test_root/stderr" \
    || fail "$description generation missed the explicit replacement gate"
  receipt_after="$(sha256sum -- "$receipt" | awk '{print $1}')"
  [[ "$receipt_after" == "$receipt_before" ]] \
    || fail "$description generation replaced the prior receipt"
  expect_failure "$description validation" "${environment[@]}" bash -c \
    "cd '$clone' && bash ops/ci/artifact_support.sh --validate-receipt '$receipt_relative'"
  grep -Fq "$marker" "$test_root/stderr" \
    || fail "$description validation missed the explicit replacement gate"
  receipt_after="$(sha256sum -- "$receipt" | awk '{print $1}')"
  [[ "$receipt_after" == "$receipt_before" ]] \
    || fail "$description validation replaced the prior receipt"
}

printf '\n# replacement-object hostile\n' >> "$clone/.cargo/config.toml"
git -C "$clone" add -- .cargo/config.toml
git -C "$clone" -c user.name='Hostile Test' -c user.email='hostile@example.invalid' \
  commit -m 'hostile replacement tree' >/dev/null
replacement_commit="$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects \
  -C "$clone" rev-parse 'HEAD^{commit}')"
replacement_tree="$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects \
  -C "$clone" rev-parse 'HEAD^{tree}')"
[[ "$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
    rev-parse "$replacement_commit:ops/ci/artifact_support.sh")" \
  == "$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
    rev-parse "$clone_head:ops/ci/artifact_support.sh")" ]] \
  || fail 'replacement fixture did not preserve the successor artifact entrypoint'
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

# Load the pinned successor helper to prove its fixed Git wrapper ignores a
# caller-selected foreign Git directory even before the entrypoint rejects it.
# shellcheck source=/dev/null
source "$ci_lib"

# A foreign Git directory can name a different commit with the exact victim
# tree. Reject every caller-selected Git authority variable before any read.
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

# A full replacement can substitute the tracked entrypoint itself. The
# successor guard works only when its bytes are already pinned/preloaded; the
# external root-owned exact materializer remains the release trust boundary.
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
  && "$(git -C "$clone" hash-object --no-filters -- ops/ci/artifact_support.sh)" \
    == "$(GIT_NO_REPLACE_OBJECTS=1 git --no-replace-objects -C "$clone" \
      rev-parse "$clone_parent:ops/ci/artifact_support.sh")" ]] \
  || fail 'full predecessor replacement did not reproduce entrypoint substitution'
if jeryu_reject_git_replacement_authority "$clone" \
  'preloaded successor artifact guard' > "$test_root/stdout" 2> "$test_root/stderr"; then
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
# authority. Both generation and validation fail before evidence can move.
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

reset_evidence
tmp="$receipt.tmp"
jq '.status = "bootstrap"' "$receipt" > "$tmp"
mv -f -- "$tmp" "$receipt"
expect_validation_failure 'bootstrap receipt'

reset_evidence
tmp="$receipt.tmp"
jq '.unexpected = true' "$receipt" > "$tmp"
mv -f -- "$tmp" "$receipt"
expect_validation_failure 'open receipt envelope'

reset_evidence
mkdir -- "$evidence_root/unexpected-subsidiary"
expect_validation_failure 'unknown artifact evidence subsidiary'

reset_evidence
inventory_physical="$inventory.physical"
mv -- "$inventory" "$inventory_physical"
ln -s -- "$(basename "$inventory_physical")" "$inventory"
expect_validation_failure 'symlinked inventory subsidiary'

reset_evidence
inventory_physical="$inventory.physical"
mv -- "$inventory" "$inventory_physical"
ln -- "$inventory_physical" "$inventory"
expect_validation_failure 'hard-linked inventory subsidiary'

reset_evidence
metadata="$evidence_root/cargo-metadata.json"
mv -- "$metadata" "$metadata.physical"
ln -s -- "$(basename "$metadata.physical")" "$metadata"
expect_validation_failure 'symlinked Cargo metadata subsidiary'

reset_evidence
sbom="$evidence_root/jeryu-cache.spdx.json"
mv -- "$sbom" "$sbom.physical"
ln -- "$sbom.physical" "$sbom"
expect_validation_failure 'hard-linked SPDX subsidiary'

reset_evidence
mv -- "$receipt" "$receipt.physical"
ln -s -- "$(basename "$receipt.physical")" "$receipt"
expect_validation_failure 'symlinked artifact receipt'

reset_evidence
mv -- "$receipt" "$receipt.physical"
ln -- "$receipt.physical" "$receipt"
expect_validation_failure 'hard-linked artifact receipt'

reset_evidence
physical_root="$clone/target/artifact-support.physical"
mv -- "$evidence_root" "$physical_root"
ln -s -- "$(basename "$physical_root")" "$evidence_root"
expect_validation_failure 'symlinked artifact evidence parent'
rm -- "$evidence_root"
mv -- "$physical_root" "$evidence_root"

reset_evidence
tmp="$inventory.tmp"
grep -Ev '  (\.cargo/|rust-toolchain\.toml$)' "$inventory" > "$tmp"
mv -f -- "$tmp" "$inventory"
inventory_sha="$(sha256sum -- "$inventory" | awk '{print $1}')"
inventory_count="$(wc -l < "$inventory" | tr -d ' ')"
tmp="$receipt.tmp"
jq --arg sha "$inventory_sha" --argjson count "$inventory_count" \
  '.inventory_sha256 = $sha | .artifact_count = $count' "$receipt" > "$tmp"
mv -f -- "$tmp" "$receipt"
expect_validation_failure 'digest-bound inventory omitting build config/toolchain'

reset_evidence
tmp="$receipt.tmp"
jq '.syft_sha256 = "0000000000000000000000000000000000000000000000000000000000000000"' \
  "$receipt" > "$tmp"
mv -f -- "$tmp" "$receipt"
expect_validation_failure 'forged governed Syft digest'

reset_evidence
cp -- "$receipt" "$test_root/noncanonical-receipt.json"
expect_failure 'noncanonical receipt path' bash -c \
  "cd '$clone' && bash ops/ci/artifact_support.sh --validate-receipt '$test_root/noncanonical-receipt.json'"

reset_evidence
receipt_before="$(sha256sum -- "$receipt" | awk '{print $1}')"
printf '\n# hostile build configuration\n' >> "$clone/.cargo/config.toml"
expect_failure 'dirty .cargo generation input' bash -c \
  "cd '$clone' && bash ops/ci/artifact_support.sh"
[[ "$(sha256sum -- "$receipt" | awk '{print $1}')" == "$receipt_before" ]] \
  || fail 'dirty .cargo input replaced the prior receipt'
git -C "$clone" restore .cargo/config.toml

printf '\n# hostile toolchain\n' >> "$clone/rust-toolchain.toml"
expect_validation_failure 'dirty rust-toolchain.toml validation input'
git -C "$clone" restore rust-toolchain.toml

expect_failure 'unknown argument' bash -c \
  "cd '$clone' && bash ops/ci/artifact_support.sh unexpected"

printf 'artifact support hostile tests ok\n'
