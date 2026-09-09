#!/usr/bin/env bash
# Transaction unit tests, not candidate qualification. Never invokes installer main.
# Fixture receipt bytes are opaque text, not any qualification receipt schema.
set -euo pipefail
umask 077
self=$(realpath -e -- "${BASH_SOURCE[0]}")
installer=${1:-$(dirname -- "$self")/install-jankurai.sh}
shift || true
[[ -f $installer && ! -L $installer && $(id -u) != 0 ]] || exit 2

# Import the actual complete named function bodies. Fail if a declaration changes
# shape or is duplicated; no maintained copy or replacement implementation exists.
functions=(die sha256_file remove_owned_scratch candidate_ancestors candidate_receipt_custody
  validate_custody_dir validate_install_root validate_install_lock require_install_lock
  create_exclusive_leaf validate_retained_leaf remove_retained_leaf rollback_target finish)
source_bodies=""
for fn in "${functions[@]}"; do
  body=$(awk -v name="$fn" '
    $0 == name "() {" {seen++; active=1}
    active {print}
    active && $0 == "}" {active=0; complete++}
    END {if (seen != 1 || complete != 1 || active) exit 1}
  ' "$installer") || { printf 'cannot import helper %s\n' "$fn" >&2; exit 2; }
  source_bodies+="$body"$'\n'
done
# shellcheck source=/dev/null
source /dev/stdin <<< "$source_bodies"
unset source_bodies body

# The imported transaction helpers consume these fixture variables.
# shellcheck disable=SC2034
run_case() {
  local scenario=$1 fixture=$2
  installer_pid=$BASHPID
  install_root=$fixture
  mkdir -m 700 -- "$install_root"
  mkdir -m 700 -- "$install_root/bin" "$install_root/receipts" "$install_root/rollback"
  install_dir=$install_root/bin
  receipt_dir=$install_root/receipts
  rollback_dir=$install_root/rollback
  exec {install_root_fd}<"$install_root"
  install_root_fd_path=/proc/$installer_pid/fd/$install_root_fd
  install_root_identity=$(stat -Lc '%d:%i:%u:%g:%a' -- "$install_root_fd_path")
  install_lock_path=$install_root/.jankurai-install.lock
  : > "$install_lock_path"
  install_lock_custody_path=$install_root_fd_path/.jankurai-install.lock
  exec {install_lock_fd}<"$install_lock_path"
  flock -x -w 2 "$install_lock_fd"
  install_lock_identity=$(stat -Lc '%d:%i:%u:%g:%a:%h' -- "/proc/$installer_pid/fd/$install_lock_fd")
  exec {install_dir_fd}<"$install_dir"
  exec {receipt_dir_fd}<"$receipt_dir"
  exec {rollback_dir_fd}<"$rollback_dir"
  install_dir_fd_path=/proc/$installer_pid/fd/$install_dir_fd
  receipt_dir_fd_path=/proc/$installer_pid/fd/$receipt_dir_fd
  target_custody_path=$install_dir_fd_path/jankurai
  printf 'new transaction bytes\n' > "$target_custody_path"
  chmod 755 "$target_custody_path"
  installed_target_identity=$(stat -Lc '%d:%i:%u:%g:%h' -- "$target_custody_path")
  previous_backup=$rollback_dir/predecessor.fixture
  printf 'predecessor bytes\n' > "$previous_backup"
  previous_sha=$(sha256_file "$previous_backup")
  exec {previous_backup_fd}<"$previous_backup"
  if [[ $scenario == first_install ]]; then previous_backup=""; previous_backup_fd=""; fi

  receipt_leaf=attempt.fixture
  receipt_path=$receipt_dir/$receipt_leaf
  printf 'opaque transaction fixture, never qualification evidence\n' > "$receipt_path"
  receipt_sha=$(sha256_file "$receipt_path")
  exec {receipt_fd}<"$receipt_path"
  receipt_install_identity=$(stat -Lc '%d:%i:%u:%g:%h' -- "/proc/$installer_pid/fd/$receipt_fd")
  printf 'unrelated existing receipt fixture\n' > "$receipt_dir/existing.fixture"
  public_candidate=1 target_replaced=1 receipt_published=1 success=0
  stage_fd="" stage_leaf="" backup_stage_fd="" backup_stage_leaf=""
  receipt_install_fd="" receipt_install_leaf="" candidate_state="" scratch=""
  # This exercises the live helper's positive control before each failure.
  validate_install_lock
  candidate_receipt_custody "$receipt_fd" "$receipt_sha" "$receipt_path"
  trap finish EXIT
  case "$scenario" in
    new_receipt|first_install) ;;
    preexisting_receipt) receipt_published=0 ;;
    unlink_failure) chmod 500 "$receipt_dir" ;;
    sync_failure)
      # Narrow syscall-failure injection: all other sync operations remain real.
      sync() {
        if [[ $# == 2 && $1 == -f && $2 == "$receipt_dir_fd_path" ]]; then return 91; fi
        command sync "$@"
      }
      ;;
    receipt_tamper)
      printf 'changed bytes\n' >> "$receipt_path"
      if candidate_receipt_custody "$receipt_fd" "$receipt_sha" "$receipt_path"; then
        printf 'tampered content admitted\n' >&2; exit 90
      fi
      ;;
    receipt_swap)
      mv -- "$receipt_path" "$receipt_dir/displaced.fixture"
      printf 'replacement belongs to another transaction\n' > "$receipt_path"
      if candidate_receipt_custody "$receipt_fd" "$receipt_sha" "$receipt_path"; then
        printf 'substituted receipt admitted\n' >&2; exit 90
      fi
      ;;
    cleanup_link)
      scratch=$install_root/build-scratch
      mkdir -m 700 -- "$scratch"
      scratch_identity=$(stat -c '%d:%i:%u' -- "$scratch")
      printf 'outside cleanup sentinel\n' > "$install_root/sentinel.fixture"
      ln -s "$install_root/sentinel.fixture" "$scratch/hostile-link"
      if remove_owned_scratch "$scratch" "$scratch_identity"; then
        printf 'linked scratch admitted\n' >&2; exit 90
      fi
      ;;
    *) exit 2 ;;
  esac
  exit 42
}

if [[ ${1:-} == --case ]]; then
  run_case "$2" "$3"
fi

base=$(mktemp -d /tmp/jeryu-candidate-transaction-test.XXXXXX)
base_identity=$(stat -c '%d:%i:%u' -- "$base")
cleanup_tests() {
  local status=$? fixture mount_point
  trap - EXIT
  [[ -d $base && ! -L $base && $(realpath -e -- "$base") == "$base" &&
     $(stat -c '%d:%i:%u' -- "$base") == "$base_identity" ]] || exit 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "$mount_point"
    [[ $mount_point != "$base" && $mount_point != "$base/"* ]] || exit 1
  done </proc/self/mountinfo
  # Restore only the explicit fixture modes/links after inspecting their paths.
  for fixture in "$base"/*; do
    [[ -d $fixture && ! -L $fixture ]] || continue
    if [[ -d $fixture/receipts && ! -L $fixture/receipts ]]; then chmod 700 "$fixture/receipts"; fi
    if [[ -L $fixture/build-scratch/hostile-link &&
          $(readlink -- "$fixture/build-scratch/hostile-link") == "$fixture/sentinel.fixture" ]]; then
      rm -f -- "$fixture/build-scratch/hostile-link"
    fi
  done
  remove_owned_scratch "$base" "$base_identity" || status=1
  exit "$status"
}
trap cleanup_tests EXIT
trap 'exit 130' INT TERM HUP
for scenario in new_receipt preexisting_receipt unlink_failure sync_failure first_install receipt_tamper receipt_swap cleanup_link; do
  fixture=$base/$scenario
  if bash "$self" "$installer" --case "$scenario" "$fixture" >"$base/$scenario.log" 2>&1; then
    status=0
  else
    status=$?
  fi
  expected_status=42
  [[ $scenario != cleanup_link ]] || expected_status=1
  if [[ $status != "$expected_status" ]]; then
    tail -n 20 "$base/$scenario.log" >&2
    printf 'FAIL %s: expected status %s, got %s\n' "$scenario" "$expected_status" "$status" >&2
    exit 1
  fi
  if [[ $scenario == first_install ]]; then
    [[ ! -e $fixture/bin/jankurai && ! -L $fixture/bin/jankurai ]]
  else
    cmp -s <(printf 'predecessor bytes\n') "$fixture/bin/jankurai"
    [[ $(stat -c %a -- "$fixture/bin/jankurai") == 755 ]]
  fi
  cmp -s <(printf 'unrelated existing receipt fixture\n') "$fixture/receipts/existing.fixture"
  case "$scenario" in
    preexisting_receipt|unlink_failure)
      cmp -s <(printf 'opaque transaction fixture, never qualification evidence\n') "$fixture/receipts/attempt.fixture" ;;
    receipt_swap)
      cmp -s <(printf 'replacement belongs to another transaction\n') "$fixture/receipts/attempt.fixture" ;;
    *) [[ ! -e $fixture/receipts/attempt.fixture && ! -L $fixture/receipts/attempt.fixture ]] ;;
  esac
  if [[ $scenario == cleanup_link ]]; then
    [[ -d $fixture/build-scratch && -L $fixture/build-scratch/hostile-link ]]
    cmp -s <(printf 'outside cleanup sentinel\n') "$fixture/sentinel.fixture"
  fi
  printf 'PASS %s\n' "$scenario"
done
printf 'PASS 8 transaction helper cases; installer_sha256=%s\n' "$(sha256_file "$installer")"
