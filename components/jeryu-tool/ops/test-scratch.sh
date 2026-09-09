#!/usr/bin/env bash
# Guard the one private scratch directory owned by a shell contract test.

record_test_scratch() {
  jeryu_test_scratch=$1
  [[ -d $jeryu_test_scratch && ! -L $jeryu_test_scratch && -O $jeryu_test_scratch &&
     $(realpath -e -- "$jeryu_test_scratch") == "$jeryu_test_scratch" ]] || return 1
  jeryu_test_scratch_identity=$(stat -c '%d:%i:%u' -- "$jeryu_test_scratch")
}

remove_test_scratch() {
  local mount_point links link target
  [[ -d $jeryu_test_scratch && ! -L $jeryu_test_scratch && -O $jeryu_test_scratch &&
     $(realpath -e -- "$jeryu_test_scratch") == "$jeryu_test_scratch" &&
     $(stat -c '%d:%i:%u' -- "$jeryu_test_scratch") == "$jeryu_test_scratch_identity" ]] || return 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "$mount_point"
    [[ $mount_point != "$jeryu_test_scratch" && $mount_point != "$jeryu_test_scratch/"* ]] || return 1
  done </proc/self/mountinfo
  links=$(find "$jeryu_test_scratch" -xdev -type l -print) || return 1
  while IFS= read -r link; do
    [[ -n $link ]] || continue
    target=$(realpath -m -- "$link") || return 1
    [[ $target == "$jeryu_test_scratch" || $target == "$jeryu_test_scratch/"* ]] || return 1
  done <<< "$links"
  [[ ! -L $jeryu_test_scratch &&
     $(stat -c '%d:%i:%u' -- "$jeryu_test_scratch") == "$jeryu_test_scratch_identity" ]] || return 1
  rm -rf --one-file-system --preserve-root=all -- "$jeryu_test_scratch"
}
