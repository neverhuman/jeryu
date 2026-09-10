#!/usr/bin/env bash
# Resolve, reproduce, and independently check exact-source split trees.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
umask 077
prepare_local='' selected_component=''
while (( $# )); do
  case $1 in
    --component)
      [[ $# -ge 2 && -z $selected_component && $2 =~ ^jeryu-[a-z-]+$ ]] || exit 2
      selected_component=$2; shift 2 ;;
    --prepare-local)
      [[ $# -ge 2 && -z $prepare_local && $2 =~ ^/[A-Za-z0-9_./-]+$ ]] || exit 2
      prepare_local=$2; shift 2 ;;
    *) printf 'usage: scripts/test-split-exports.sh [--component NAME] [--prepare-local SOURCE]\n' >&2; exit 2 ;;
  esac
done
if [[ -n $prepare_local ]]; then
  # shellcheck source=scripts/source-build.sh
  source "$root/scripts/source-build.sh"
  git() { split_source_git "$PWD" "$@"; }
fi
source_commit=$(git rev-parse HEAD)
source_tree=$(git rev-parse 'HEAD^{tree}')
source_status=$(git status --porcelain=v1 --untracked-files=all) || {
  printf 'could not inspect split qualification source state\n' >&2; exit 1;
}
[[ -z $source_status ]] || {
  printf 'commit source changes before split qualification\n' >&2; exit 1;
}
component_names=$(git ls-tree -d --name-only "$source_commit:components") || {
  printf 'could not read split component inventory\n' >&2; exit 1;
}
[[ -n $component_names ]] || { printf 'split component inventory is empty\n' >&2; exit 1; }
mapfile -t components <<< "$component_names"
declare -A known_components=()
for component in "${components[@]}"; do
  [[ $component =~ ^jeryu-[a-z-]+$ && -z ${known_components[$component]:-} ]] || {
    printf 'invalid or duplicate split component\n' >&2; exit 1;
  }
  known_components[$component]=1
done
if [[ -n $selected_component ]]; then
  [[ ${known_components[$selected_component]:-} == 1 ]] || exit 2
  components=("$selected_component")
fi
prepare_before=''
if [[ -n $prepare_local ]]; then
  # shellcheck source=scripts/source-build.sh
  source "$root/scripts/source-build.sh"
  prepare_before=$(split_source_snapshot "$prepare_local" "$source_commit") || exit 1
  [[ $(split_source_git "$prepare_local" rev-parse 'HEAD^{tree}') == "$source_tree" ]] || exit 1
fi
cargo build --locked -p jeryu-split-tool --bin jeryu-split
tool="$(realpath -m "${CARGO_TARGET_DIR:-$root/target}")/debug/jeryu-split"
for directory in "$root/target" "$root/target/split-evidence" "$root/target/split-evidence/$source_commit"; do
  if [[ ! -e $directory && ! -L $directory ]]; then mkdir -m 0700 -- "$directory"; fi
  [[ -d $directory && ! -L $directory && -O $directory &&
     $(realpath -e -- "$directory") == "$directory" ]] || {
    printf 'split evidence directory is not physical and owned\n' >&2; exit 1;
  }
done
evidence=$(mktemp -d "$root/target/split-evidence/$source_commit/attempt.XXXXXXXX")
evidence_identity=$(stat -c '%d:%i:%u:%g:%a' -- "$evidence")
# shellcheck source=tests/scratch.sh
source "$root/tests/scratch.sh"
scratch=$(mktemp -d -t jeryu-split-proof.XXXXXXXX)
jeryu_record_test_scratch "$scratch"
cleanup() {
  local result=$? observed
  if [[ -n $prepare_local ]]; then
    observed=$(split_source_snapshot "$prepare_local" "$source_commit") || result=1
    [[ $observed == "$prepare_before" ]] || result=1
  fi
  if (( result != 0 )); then
    printf 'Retaining split scratch after failed verification: %s\n' "$scratch" >&2
    exit "$result"
  fi
  if ! jeryu_remove_test_scratch; then
    printf 'retaining changed, linked, or mounted split scratch: %s\n' "$scratch" >&2
    result=1
  fi
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
printf '%s\n' "$scratch" > "$evidence/scratch-path.txt"
jq -n --arg commit "$source_commit" --arg tree "$source_tree" \
  '{source_commit:$commit,source_tree:$tree,source_state:"clean",publication_qualified:false}' > "$evidence/source.json"
if [[ -n $prepare_local ]]; then
  printf '%s\n' "$prepare_local" > "$evidence/local-source-path.txt"
  printf '%s\n' "$prepare_before" > "$evidence/local-source.snapshot"
  printf 'local-source-preparation; public origin unproven\n' > "$evidence/transport.txt"
fi
printf 'Split evidence: %s\n' "$evidence"
failed=0
for component in "${components[@]}"; do
  printf 'Qualifying %s from %s\n' "$component" "$source_commit"
  if ! bash -euo pipefail -s -- "$root" "$tool" "$component" "$source_commit" "$evidence" "$scratch" "$prepare_local" \
    > "$evidence/$component.log" 2>&1 <<'CHECK'
    root=$1 tool=$2 component=$3 source_commit=$4 evidence=$5 scratch=$6 prepare_local=$7
    local_args=()
    if [[ -n $prepare_local ]]; then
      local_args=(--prepare-local "$prepare_local")
      # shellcheck source=scripts/source-build.sh
      source "$root/scripts/source-build.sh"
      git() { split_source_git "$PWD" "$@"; }
    fi
    cd "$root"
    for attempt in 1 2; do
      "$tool" export-tree --component "$component" --source "$source_commit" --resolve-lock "${local_args[@]}" > "$evidence/$component-$attempt.json"
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
    checked_commit=$(git rev-parse HEAD)
    checked_tree=$(git rev-parse 'HEAD^{tree}')
    checked_status=$(git status --porcelain=v1 --untracked-files=all)
    [[ $checked_commit == "$commit" && $checked_tree == "$tree" && -z $checked_status ]] || {
      printf 'split checkout does not match the clean exported commit\n' >&2; exit 1;
    }
    export CARGO_TARGET_DIR="$root/target/split-build"
    unset JERYU_WEB_DIST JERYU_REQUIRE_WEB
    bash scripts/split-ci.sh ordinary "${local_args[@]}"
    checked_commit=$(git rev-parse HEAD)
    checked_tree=$(git rev-parse 'HEAD^{tree}')
    checked_status=$(git status --porcelain=v1 --untracked-files=all)
    [[ $checked_commit == "$commit" && $checked_tree == "$tree" && -z $checked_status ]] || {
      printf 'split checkout changed during independent checks\n' >&2; exit 1;
    }
CHECK
  then
    printf 'Split verification failed: %s; see %s/%s.log\n' "$component" "$evidence" "$component" >&2
    failed=1
  else
    printf 'Split checks completed: %s\n' "$component"
  fi
done
checked_commit=$(git rev-parse HEAD)
checked_tree=$(git rev-parse 'HEAD^{tree}')
source_status=$(git status --porcelain=v1 --untracked-files=all) || {
  printf 'could not recheck split qualification source state\n' >&2; exit 1;
}
[[ $checked_commit == "$source_commit" && $checked_tree == "$source_tree" && -z $source_status ]] || {
  printf 'source changed during split qualification\n' >&2; exit 1;
}
[[ -d $evidence && ! -L $evidence && $(realpath -e -- "$evidence") == "$evidence" &&
   $(stat -c '%d:%i:%u:%g:%a' -- "$evidence") == "$evidence_identity" ]] || {
  printf 'split evidence directory changed during execution\n' >&2; exit 1;
}
(( failed == 0 )) || exit "$failed"
if [[ -n $prepare_local ]]; then
  [[ $(split_source_snapshot "$prepare_local" "$source_commit") == "$prepare_before" ]] || exit 1
fi
jeryu_remove_test_scratch
trap - EXIT
if (( failed == 0 )); then
  printf 'Split export checks passed for %s components: source=%s evidence=%s; publication remains unqualified.\n' \
    "${#components[@]}" "$source_commit" "$evidence"
fi
exit "$failed"
