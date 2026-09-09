#!/usr/bin/env bash
# Test-only source-install transaction. Originals remain in verified private recovery.
# shellcheck source=tests/scratch.sh
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/scratch.sh"

declare -A jeryu_install_directories=() jeryu_install_paths=() jeryu_install_expected=()
declare -A jeryu_install_backups=() jeryu_install_originals=() jeryu_install_fds=()
declare -A jeryu_install_content=()
jeryu_install_active=0
jeryu_install_ready=0
jeryu_install_stage=''

jeryu_install_directory_state() {
  [[ $1 == /* && -d $1 && ! -L $1 && -O $1 && $(realpath -e -- "$1") == "$1" ]] || return 1
  stat -c '%d:%i:%u:%g:%a' -- "$1"
}

jeryu_install_no_mounts() {
  local mount_point
  [[ -r /proc/self/mountinfo ]] || return 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "$mount_point"
    [[ $mount_point != "$1" && $mount_point != "$1/"* ]] || return 1
  done </proc/self/mountinfo
}

jeryu_install_file_state() {
  local path=$1 links=${2:-1} before after descriptor digest held
  if [[ ! -e $path && ! -L $path ]]; then printf 'absent\n'; return; fi
  [[ $path == /* && -f $path && ! -L $path && -O $path &&
     $(realpath -e -- "$path") == "$path" ]] || return 1
  [[ $links == original || $(stat -c '%h' -- "$path") == 1 ]] || return 1
  before=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$path") || return 1
  exec {descriptor}<"$path" || return 1
  held="/proc/self/fd/$descriptor"
  if [[ $(stat -Lc '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$held") != "$before" ]]; then
    exec {descriptor}<&-; return 1
  fi
  digest=$(sha256sum <"$held") || { exec {descriptor}<&-; return 1; }
  after=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$path") || {
    exec {descriptor}<&-; return 1;
  }
  [[ $before == "$after" && ! -L $path &&
     $(stat -Lc '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$held") == "$before" ]] || {
    exec {descriptor}<&-; return 1;
  }
  exec {descriptor}<&-
  printf '%s|%s\n' "$before" "${digest%% *}"
}

jeryu_install_private_tree() (
  set -o pipefail
  local path
  jeryu_install_no_mounts "$scratch" || return 1
  find "$scratch" -xdev -print0 |
  while IFS= read -r -d '' path; do
    [[ ! -L $path && -O $path ]] || return 1
    if [[ -f $path ]]; then
      [[ $(stat -c '%h' -- "$path") == 1 ]] || return 1
    else
      [[ -d $path ]] || return 1
    fi
  done
)

jeryu_install_assert() {
  local directory name actual links
  [[ ! -L $jeryu_install_tmp_parent && $(realpath -e -- "$jeryu_install_tmp_parent") == "$jeryu_install_tmp_parent" &&
     $(stat -c '%d:%i:%u:%g:%a' -- "$jeryu_install_tmp_parent") == "$jeryu_install_tmp_parent_state" ]] || return 1
  for directory in "${!jeryu_install_directories[@]}"; do
    actual=$(jeryu_install_directory_state "$directory") || return 1
    [[ $actual == "${jeryu_install_directories[$directory]}" ]] || return 1
  done
  jeryu_install_no_mounts "$root" || return 1
  jeryu_install_private_tree || return 1
  for name in artifact receipt fixture; do
    links=1
    if [[ $name == artifact && ${jeryu_install_detached:-0} == 0 ]]; then links=original; fi
    actual=$(jeryu_install_file_state "${jeryu_install_paths[$name]}" "$links") || return 1
    [[ $actual == "${jeryu_install_expected[$name]}" ]] || return 1
  done
  for name in artifact receipt; do
    actual=$(jeryu_install_file_state "$scratch/$name") || return 1
    [[ $actual == "${jeryu_install_backups[$name]}" ]] || return 1
  done
}

jeryu_install_capture() {
  local name=$1 path=${jeryu_install_paths[$1]} descriptor held state content
  state=$(jeryu_install_file_state "$path" original) || return 1
  [[ $state != absent && $state == "${jeryu_install_expected[$name]}" ]] || return 1
  exec {descriptor}<"$path" || return 1
  jeryu_install_fds[$name]=$descriptor
  held="/proc/self/fd/$descriptor"
  [[ $(stat -Lc '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$held") == "${state%|*}" ]] || return 1
  content=$(stat -Lc '%u:%g:%a:%s:%y' -- "$held") || return 1
  cp -L --preserve=all --reflink=never -- "$held" "$scratch/$name" || return 1
  jeryu_install_backups[$name]=$(jeryu_install_file_state "$scratch/$name") || return 1
  [[ ${jeryu_install_backups[$name]##*|} == "${state##*|}" &&
     $(stat -c '%u:%g:%a:%s:%y' -- "$scratch/$name") == "$content" &&
     $(jeryu_install_file_state "$path" original) == "$state" &&
     $(stat -Lc '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$held") == "${state%|*}" ]] || return 1
  jeryu_install_originals[$name]=$state
  jeryu_install_content[$name]=$content
  jeryu_install_expected[$name]=$state
  printf 'original %s %q %q\n' "$name" "$path" "$state" >>"$scratch/recovery.log"
  printf 'recovery %s %q %q\n' "$name" "$scratch/$name" "${jeryu_install_backups[$name]}" >>"$scratch/recovery.log"
}

jeryu_install_restore() {
  local name=$1 staged state staged_identity stage_fd
  [[ $name == artifact || $name == receipt ]] || return 1
  [[ -z $jeryu_install_stage ]] || return 1
  jeryu_install_assert || return 1
  # The held directory anchors staging/promotion if an ancestor is renamed.
  staged=$(mktemp "/proc/self/fd/$jeryu_install_release_fd/.source-install-restore.XXXXXXXX") || return 1
  jeryu_install_stage="$root/target/release/${staged##*/}"
  printf 'restore-stage %s %q\n' "$name" "$jeryu_install_stage" >>"$scratch/recovery.log"
  state=$(jeryu_install_file_state "$jeryu_install_stage") || return 1
  exec {stage_fd}<>"$staged" || return 1
  [[ $(stat -Lc '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "/proc/self/fd/$stage_fd") == "${state%|*}" ]] || {
    exec {stage_fd}>&-; return 1;
  }
  cp --preserve=all --reflink=never -- "$scratch/$name" "/proc/self/fd/$stage_fd" || {
    exec {stage_fd}>&-; return 1;
  }
  exec {stage_fd}>&-
  state=$(jeryu_install_file_state "$jeryu_install_stage") || return 1
  [[ ${state##*|} == "${jeryu_install_originals[$name]##*|}" &&
     $(stat -c '%u:%g:%a:%s:%y' -- "$jeryu_install_stage") == "${jeryu_install_content[$name]}" ]] || return 1
  staged_identity=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y' -- "$jeryu_install_stage") || return 1
  jeryu_install_assert || return 1
  [[ $(jeryu_install_file_state "$jeryu_install_stage") == "$state" ]] || return 1
  mv -fT -- "$staged" "/proc/self/fd/$jeryu_install_release_fd/${jeryu_install_paths[$name]##*/}" || return 1
  [[ $(stat -c '%d:%i:%u:%g:%a:%h:%s:%y' -- "${jeryu_install_paths[$name]}") == "$staged_identity" ]] || return 1
  jeryu_install_stage=''
  if [[ $name == artifact ]]; then jeryu_install_detached=1; fi
  jeryu_install_expected[$name]=$(jeryu_install_file_state "${jeryu_install_paths[$name]}") || return 1
  [[ ${jeryu_install_expected[$name]##*|} == "${jeryu_install_originals[$name]##*|}" &&
     $(stat -c '%u:%g:%a:%s:%y' -- "${jeryu_install_paths[$name]}") == "${jeryu_install_content[$name]}" ]] || return 1
}

jeryu_install_begin() {
  local directory name tmp_parent
  root=$1
  for directory in "$root" "$root/target" "$root/target/release"; do
    jeryu_install_directories[$directory]=$(jeryu_install_directory_state "$directory") || return 1
  done
  jeryu_install_no_mounts "$root" || return 1
  artifact="$root/target/release/jeryu"
  receipt="$root/target/release/jeryu.source-build"
  fixture="$root/.source-install-fixture-$$"
  jeryu_install_paths=([artifact]="$artifact" [receipt]="$receipt" [fixture]="$fixture")
  for name in artifact receipt fixture; do
    jeryu_install_expected[$name]=$(jeryu_install_file_state "${jeryu_install_paths[$name]}" \
      "$([[ $name == artifact ]] && printf original || printf 1)") || return 1
  done
  [[ ${jeryu_install_expected[fixture]} == absent &&
     ${jeryu_install_expected[artifact]} != absent && ${jeryu_install_expected[receipt]} != absent ]] || return 1
  tmp_parent=$(realpath -e -- "${TMPDIR:-/tmp}") || return 1
  [[ -d $tmp_parent && ! -L $tmp_parent ]] || return 1
  jeryu_install_tmp_parent=$tmp_parent
  jeryu_install_tmp_parent_state=$(stat -c '%d:%i:%u:%g:%a' -- "$tmp_parent") || return 1
  scratch=$(mktemp -d "$tmp_parent/jeryu-source-install.XXXXXXXX") || return 1
  jeryu_install_active=1
  [[ $(stat -c '%a' -- "$scratch") == 700 ]] || return 1
  jeryu_record_test_scratch "$scratch" || return 1
  jeryu_install_directories[$scratch]=$(jeryu_install_directory_state "$scratch") || return 1
  exec {jeryu_install_root_fd}<"$root" || return 1
  [[ $(stat -Lc '%d:%i:%u:%g:%a' -- "/proc/self/fd/$jeryu_install_root_fd") == \
     "${jeryu_install_directories[$root]}" ]] || return 1
  exec {jeryu_install_release_fd}<"$root/target/release" || return 1
  [[ $(stat -Lc '%d:%i:%u:%g:%a' -- "/proc/self/fd/$jeryu_install_release_fd") == \
     "${jeryu_install_directories[$root/target/release]}" ]] || return 1
  for directory in "${!jeryu_install_directories[@]}"; do
    printf 'directory %q %q\n' "$directory" "${jeryu_install_directories[$directory]}" >>"$scratch/recovery.log"
  done
  printf 'temporary-parent %q %q\n' "$jeryu_install_tmp_parent" "$jeryu_install_tmp_parent_state" >>"$scratch/recovery.log"
  for name in artifact receipt; do jeryu_install_capture "$name" || return 1; done
  jeryu_install_ready=1
  jeryu_install_detached=0
  # Never tamper with a Cargo inode shared with target/release/deps.
  jeryu_install_restore artifact || return 1
  jeryu_install_restore receipt || return 1
}

jeryu_install_change() {
  local operation=$1 name descriptor held anchored
  jeryu_install_assert || return 1
  case "$operation" in
    artifact-tamper|receipt-corrupt)
      name=artifact
      if [[ $operation == receipt-corrupt ]]; then name=receipt; fi
      anchored="/proc/self/fd/$jeryu_install_release_fd/${jeryu_install_paths[$name]##*/}"
      exec {descriptor}>>"$anchored" || return 1
      held="/proc/self/fd/$descriptor"
      [[ $(stat -Lc '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$held") == \
         "${jeryu_install_expected[$name]%|*}" ]] || { exec {descriptor}>&-; return 1; }
      if [[ $operation == artifact-tamper ]]; then
        printf 'tampered\n' >&"$descriptor" || { exec {descriptor}>&-; return 1; }
      else
        truncate -s 0 -- "$held" && printf 'invalid receipt\n' >&"$descriptor" || {
          exec {descriptor}>&-; return 1;
        }
      fi
      exec {descriptor}>&-
      ;;
    fixture-create)
      name=fixture
      anchored="/proc/self/fd/$jeryu_install_root_fd/${fixture##*/}"
      (set -o noclobber; printf 'source changed\n' >"$anchored") || return 1
      ;;
    fixture-remove|receipt-remove)
      name=fixture
      if [[ $operation == receipt-remove ]]; then name=receipt; fi
      anchored="/proc/self/fd/$jeryu_install_root_fd/${fixture##*/}"
      if [[ $name == receipt ]]; then
        anchored="/proc/self/fd/$jeryu_install_release_fd/${receipt##*/}"
      fi
      rm -- "$anchored" || return 1
      ;;
    *) return 1 ;;
  esac
  jeryu_install_expected[$name]=$(jeryu_install_file_state "${jeryu_install_paths[$name]}") || return 1
}

jeryu_install_finish() {
  local status=$1 name descriptor digest
  [[ $jeryu_install_active == 1 ]] || return "$status"
  if [[ $jeryu_install_ready != 1 ]] || ! jeryu_install_assert; then
    status=1
  else
    if [[ ${jeryu_install_expected[fixture]} != absent ]]; then
      jeryu_install_change fixture-remove || status=1
    fi
    jeryu_install_restore artifact || status=1
    jeryu_install_restore receipt || status=1
    for name in artifact receipt; do
      descriptor=${jeryu_install_fds[$name]}
      digest=$(sha256sum <"/proc/self/fd/$descriptor") || status=1
      [[ ${digest%% *} == "${jeryu_install_originals[$name]##*|}" ]] || status=1
    done
  fi
  if [[ $status == 0 && -z $jeryu_install_stage ]] &&
    jeryu_install_assert && jeryu_install_private_tree && jeryu_remove_test_scratch; then
    jeryu_install_active=0
    for descriptor in "${jeryu_install_fds[@]}" "$jeryu_install_release_fd" "$jeryu_install_root_fd"; do
      exec {descriptor}<&-
    done
    return 0
  fi
  printf 'source-install test failed; retained recovery: %s; restore stage: %s\n' \
    "$scratch" "${jeryu_install_stage:-none}" >&2
  return 1
}
