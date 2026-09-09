#!/usr/bin/env bash
# Resolve, reproduce, and independently check exact-source split trees.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
umask 077
source_commit=$(git rev-parse HEAD)
components=()
case $# in
  0) mapfile -t components < <(git ls-tree -d --name-only "$source_commit:components") ;;
  2)
    [[ $1 == --component && $2 =~ ^jeryu-[a-z-]+$ ]] || { printf 'invalid component argument\n' >&2; exit 2; }
    components=("$2")
    ;;
  *) printf 'usage: scripts/test-split-exports.sh [--component NAME]\n' >&2; exit 2 ;;
esac
if ! git diff --quiet || ! git diff --cached --quiet || [[ -n $(git ls-files --others --exclude-standard) ]]; then
  printf 'commit source changes before split qualification\n' >&2; exit 1
fi
cargo build --locked -p jeryu-split-tool --bin jeryu-split
tool="$(realpath -m "${CARGO_TARGET_DIR:-$root/target}")/debug/jeryu-split"
evidence="$root/target/split-evidence/$source_commit"
mkdir -p "$evidence"
scratch=$(mktemp -d)
scratch_identity=$(stat -c '%d:%i' -- "$scratch")
cleanup() {
  local result=$? mounts
  mounts=$(findmnt -rn -o TARGET) || { printf 'cannot inspect mounts; retaining %s\n' "$scratch" >&2; exit 1; }
  if [[ -L "$scratch" || ! -d "$scratch" || $(realpath -e -- "$scratch") != "$scratch" \
        || $(stat -c '%d:%i' -- "$scratch") != "$scratch_identity" ]] \
      || awk -v root="$scratch" '$0 == root || index($0, root "/") == 1 {found=1} END {exit !found}' <<< "$mounts"; then
    printf 'retaining replaced or mounted split scratch: %s\n' "$scratch" >&2
    exit 1
  fi
  find "$scratch" -xdev -type l -print >&2
  rm -rf --one-file-system --preserve-root=all -- "$scratch"
  exit "$result"
}
trap cleanup EXIT
failed=0
for component in "${components[@]}"; do
  printf 'Qualifying %s from %s\n' "$component" "$source_commit"
  if ! bash -euo pipefail -s -- "$root" "$tool" "$component" "$source_commit" "$evidence" "$scratch" \
    > "$evidence/$component.log" 2>&1 <<'CHECK'
    root=$1 tool=$2 component=$3 source_commit=$4 evidence=$5 scratch=$6
    cd "$root"
    for attempt in 1 2; do
      "$tool" export-tree --component "$component" --source "$source_commit" --resolve-lock > "$evidence/$component-$attempt.json"
    done
    cmp "$evidence/$component-1.json" "$evidence/$component-2.json"
    tree=$(jq -er .tree "$evidence/$component-1.json")
    export GIT_AUTHOR_NAME='Jeryu split verification' GIT_AUTHOR_EMAIL=split@jeryu.invalid
    export GIT_COMMITTER_NAME="$GIT_AUTHOR_NAME" GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"
    export GIT_AUTHOR_DATE=2000-01-01T00:00:00Z
    export GIT_COMMITTER_DATE=$GIT_AUTHOR_DATE
    commit=$(git -c commit.gpgsign=false commit-tree "$tree" -m 'Disposable independent split verification')
    checkout="$scratch/$component"
    git clone --no-local --no-checkout --quiet "$root" "$checkout"
    git -C "$checkout" fetch --quiet --no-tags "$root" "$commit"
    git -C "$checkout" -c core.hooksPath=/dev/null checkout --quiet --detach "$commit"
    cd "$checkout"
    export CARGO_TARGET_DIR="$root/target/split-build"
    unset JERYU_WEB_DIST JERYU_REQUIRE_WEB
    bash scripts/split-ci.sh ordinary
    git diff --exit-code
    git diff --cached --exit-code
CHECK
  then
    printf 'Split verification failed: %s; see %s/%s.log\n' "$component" "$evidence" "$component" >&2
    failed=1
  else
    printf 'Split verification passed: %s\n' "$component"
  fi
done
exit "$failed"
