#!/usr/bin/env bash
# Synthetic Git/Cargo-discovery fixtures; no compiler, dependency fetch or auditor.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
helper=${1:-$root/ops/ci/workspace-lock.sh}
# shellcheck source=ops/ci/workspace-lock.sh
source "$helper"
umask 077
scratch=$(mktemp -d -t jeryu-deploy-lock-tests.XXXXXXXX)
identity=$(stat -c '%d:%i:%u:%g:%a' -- "$scratch")
cleanup() {
  local status=$? mount_point unexpected
  if (( status != 0 )); then
    printf 'retaining failed workspace-lock fixtures: %s\n' "$scratch" >&2; return "$status"
  fi
  [[ -d $scratch && ! -L $scratch && $(realpath -e -- "$scratch") == "$scratch" &&
     $(stat -c '%d:%i:%u:%g:%a' -- "$scratch") == "$identity" ]] || return 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "$mount_point"
    [[ $mount_point != "$scratch" && $mount_point != "$scratch/"* ]] || return 1
  done </proc/self/mountinfo || return 1
  unexpected=$(find -P "$scratch" -xdev \( -type l -o \( -type f ! -links 1 \) \
    -o \( ! -type d ! -type f \) \) -print -quit) || return 1
  [[ -z $unexpected ]] || return 1
  rm -rf --one-file-system --preserve-root=all -- "$scratch"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$scratch/bin"
cat > "$scratch/bin/cargo" <<'MOCK'
#!/bin/bash
set -euo pipefail
[[ $# == 6 && $1 == locate-project && $2 == --workspace &&
   $3 == --message-format && $4 == plain && $5 == --manifest-path &&
   $6 == "$EXPECTED_MEMBER" ]] || exit 95
(( CARGO_STATUS == 0 )) || exit "$CARGO_STATUS"
printf '%s\n' "$WORKSPACE_MANIFEST"
MOCK
chmod 0700 "$scratch/bin/cargo"
export PATH="$scratch/bin:$PATH" CARGO_STATUS=0 EXPECTED_MEMBER='' WORKSPACE_MANIFEST=''
cases=0
assert_exit() {
  local label=$1 expected=$2 actual=0
  shift 2
  "$@" > "$scratch/output" 2>&1 || actual=$?
  [[ $actual == "$expected" ]] || {
    printf 'workspace-lock case %s expected=%s observed=%s\n' "$label" "$expected" "$actual" >&2
    return 1
  }
  cases=$((cases + 1))
}
seed() {
  local repository=$1 component=$2
  mkdir -p "$component/crates/jeryu-api"
  env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    /usr/bin/git -c init.defaultBranch=main init --quiet --template= "$repository"
  printf 'synthetic member\n' > "$component/crates/jeryu-api/Cargo.toml"
  printf 'synthetic workspace\n' > "$repository/Cargo.toml"
  printf 'synthetic lock\n' > "$repository/Cargo.lock"
}
for layout in standalone monorepo; do
  repository="$scratch/$layout repository"
  component=$repository
  [[ $layout != monorepo ]] || component="$repository/components/jeryu-deploy"
  seed "$repository" "$component"
  EXPECTED_MEMBER="$component/crates/jeryu-api/Cargo.toml"
  WORKSPACE_MANIFEST="$repository/Cargo.toml"
  assert_exit record 0 jeryu_deploy_record_workspace_lock "$component"
  assert_exit unchanged 0 jeryu_deploy_assert_workspace_lock_unchanged
  printf 'mutated lock\n' > "$repository/Cargo.lock"
  assert_exit lock_content 1 jeryu_deploy_assert_workspace_lock_unchanged
  assert_exit rerecord 0 jeryu_deploy_record_workspace_lock "$component"
  # Same bytes at a new inode must still be rejected.
  cat "$repository/Cargo.lock" > "$repository/replacement"
  mv "$repository/replacement" "$repository/Cargo.lock"
  assert_exit lock_replaced 1 jeryu_deploy_assert_workspace_lock_unchanged
  assert_exit rerecord_replacement 0 jeryu_deploy_record_workspace_lock "$component"
  mv "$repository/Cargo.lock" "$repository/saved-lock"
  assert_exit lock_missing 1 jeryu_deploy_assert_workspace_lock_unchanged
  ln -s saved-lock "$repository/Cargo.lock"
  assert_exit lock_symlink 1 jeryu_deploy_record_workspace_lock "$component"
  [[ -L $repository/Cargo.lock && $(readlink "$repository/Cargo.lock") == saved-lock ]]
  rm -- "$repository/Cargo.lock"
  mv "$repository/saved-lock" "$repository/Cargo.lock"
  ln "$repository/Cargo.lock" "$repository/hardlink"
  assert_exit lock_hardlink 1 jeryu_deploy_record_workspace_lock "$component"
  [[ ! -L $repository/hardlink && $repository/hardlink -ef $repository/Cargo.lock ]]
  rm -- "$repository/hardlink"
  assert_exit record_before_manifest 0 jeryu_deploy_record_workspace_lock "$component"
  printf 'mutated workspace\n' > "$repository/Cargo.toml"
  assert_exit manifest_content 1 jeryu_deploy_assert_workspace_lock_unchanged
  assert_exit record_before_member 0 jeryu_deploy_record_workspace_lock "$component"
  printf 'mutated member\n' > "$EXPECTED_MEMBER"
  assert_exit member_content 1 jeryu_deploy_assert_workspace_lock_unchanged
  CARGO_STATUS=3
  assert_exit cargo_discovery_failure 1 jeryu_deploy_record_workspace_lock "$component"
  assert_exit failed_record_clears_prior 1 jeryu_deploy_assert_workspace_lock_unchanged
  CARGO_STATUS=0 WORKSPACE_MANIFEST="$repository/other/Cargo.toml"
  assert_exit cargo_wrong_workspace 1 jeryu_deploy_record_workspace_lock "$component"
  WORKSPACE_MANIFEST=''
  assert_exit cargo_empty_workspace 1 jeryu_deploy_record_workspace_lock "$component"
  WORKSPACE_MANIFEST="$repository/Cargo.toml"
  # Ambient Git root injection must not redirect read-only discovery.
  export GIT_DIR="$scratch/does-not-exist" GIT_WORK_TREE="$scratch/does-not-exist"
  assert_exit git_environment_ignored 0 jeryu_deploy_record_workspace_lock "$component"
  unset GIT_DIR GIT_WORK_TREE
done
repository="$scratch/foreign repository"
component="$repository/components/jeryu-other"
seed "$repository" "$component"
EXPECTED_MEMBER="$component/crates/jeryu-api/Cargo.toml"
WORKSPACE_MANIFEST="$repository/Cargo.toml"
assert_exit wrong_component_layout 1 jeryu_deploy_record_workspace_lock "$component"
mkdir "$scratch/no-git"
assert_exit missing_git_root 1 jeryu_deploy_record_workspace_lock "$scratch/no-git"
ln -s "$repository" "$scratch/repository-link"
assert_exit component_symlink 1 jeryu_deploy_record_workspace_lock "$scratch/repository-link"
[[ -L $scratch/repository-link && $(readlink "$scratch/repository-link") == "$repository" ]]
rm -- "$scratch/repository-link"
cleanup
trap - EXIT
printf 'Deploy workspace lock: %s synthetic cases passed; no real CI executed\n' "$cases"
