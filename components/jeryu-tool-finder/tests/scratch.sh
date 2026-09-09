#!/usr/bin/env bash
# Finder test scratch custody; also available in standalone component exports.

jeryu_record_test_scratch() {
  local directory=$1
  [[ $directory == /* && -d $directory && ! -L $directory && -O $directory &&
     $(realpath -e -- "$directory") == "$directory" ]] || return 1
  jeryu_test_scratch=$directory
  jeryu_test_scratch_identity=$(stat -c '%d:%i:%u:%g:%a' -- "$directory") || return 1
}

jeryu_remove_test_scratch() (
  set -o pipefail
  local mount_point link target
  [[ -n ${jeryu_test_scratch:-} && -n ${jeryu_test_scratch_identity:-} &&
     -d $jeryu_test_scratch && ! -L $jeryu_test_scratch && -O $jeryu_test_scratch &&
     $(realpath -e -- "$jeryu_test_scratch") == "$jeryu_test_scratch" &&
     $(stat -c '%d:%i:%u:%g:%a' -- "$jeryu_test_scratch") == "$jeryu_test_scratch_identity" ]] || return 1
  [[ -r /proc/self/mountinfo ]] || return 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "$mount_point"
    [[ $mount_point != "$jeryu_test_scratch" && $mount_point != "$jeryu_test_scratch/"* ]] || return 1
  done </proc/self/mountinfo || return 1
  # Inspect every link without following it; permit only targets inside this scratch.
  find "$jeryu_test_scratch" -xdev -type l -print0 |
    while IFS= read -r -d '' link; do
      target=$(realpath -m -- "$link") || return 1
      [[ $target == "$jeryu_test_scratch" || $target == "$jeryu_test_scratch/"* ]] || return 1
    done || return 1
  [[ ! -L $jeryu_test_scratch &&
     $(realpath -e -- "$jeryu_test_scratch") == "$jeryu_test_scratch" &&
     $(stat -c '%d:%i:%u:%g:%a' -- "$jeryu_test_scratch") == "$jeryu_test_scratch_identity" ]] || return 1
  rm -rf --one-file-system --preserve-root=all -- "$jeryu_test_scratch"
)
