#!/usr/bin/env bash
# Synthetic transactions only: no release binary, installer, Cargo or source clone runs.
set -euo pipefail
umask 077
test_code_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
# shellcheck source=tests/source-install-transaction.sh
source "$test_code_root/tests/source-install-transaction.sh"
test_outer=$(mktemp -d /tmp/jeryu-source-install-hostiles.XXXXXXXX)
jeryu_record_test_scratch "$test_outer"
test_cleanup() {
  local status=$?
  jeryu_remove_test_scratch || status=1
  exit "$status"
}
trap test_cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
fail() { printf 'source-install transaction test failed: %s\n' "$*" >&2; exit 1; }
reject() {
  if ("$@") >"$test_outer/rejected.out" 2>&1; then fail "accepted $1"; fi
}
new_fixture() {
  root="$test_outer/$1"
  mkdir -p "$root/target/release" "$root/tmp"
  printf 'synthetic binary\n' >"$root/target/release/jeryu"
  printf 'synthetic receipt\n' >"$root/target/release/jeryu.source-build"
  chmod 0755 "$root/target/release/jeryu"
  chmod 0600 "$root/target/release/jeryu.source-build"
  export TMPDIR="$root/tmp"
}
original_pair() {
  [[ $(<"$root/target/release/jeryu") == 'synthetic binary' &&
     $(<"$root/target/release/jeryu.source-build") == 'synthetic receipt' &&
     $(stat -c '%a' "$root/target/release/jeryu") == 755 &&
     $(stat -c '%a' "$root/target/release/jeryu.source-build") == 600 ]] ||
    fail 'original bytes/modes were not restored'
}
recovery_pair() {
  : "${scratch:?}"
  [[ $(<"$scratch/artifact") == 'synthetic binary' &&
     $(<"$scratch/receipt") == 'synthetic receipt' ]] ||
    fail 'recovery copies were lost or changed'
}
begin() {
  jeryu_install_begin "$root" || fail 'fixture admission failed'
  : "${scratch:?}" "${artifact:?}" "${fixture:?}"
  : "${jeryu_install_expected[artifact]:?}" "${jeryu_install_backups[artifact]:?}"
}

(
  new_fixture success
  begin
  jeryu_install_change artifact-tamper
  jeryu_install_restore artifact
  jeryu_install_change fixture-create
  jeryu_install_change fixture-remove
  jeryu_install_change receipt-corrupt
  jeryu_install_change receipt-remove
  jeryu_install_restore receipt
  jeryu_install_finish 0
  original_pair
  [[ ! -e $scratch && ! -L $scratch ]] || fail 'successful scratch remained'
)
(
  new_fixture cargo-hardlink
  mkdir "$root/target/release/deps"
  ln "$root/target/release/jeryu" "$root/target/release/deps/build-alias"
  original_inode=$(stat -c '%d:%i' "$root/target/release/jeryu")
  original_metadata=$(stat -c '%u:%g:%a:%s:%y' "$root/target/release/jeryu")
  begin
  [[ $(stat -c '%h' "$artifact") == 1 &&
     $(stat -c '%d:%i' "$artifact") != "$original_inode" ]] || fail 'Cargo artifact was not detached'
  jeryu_install_change artifact-tamper
  [[ $(<"$root/target/release/deps/build-alias") == 'synthetic binary' &&
     $(stat -c '%d:%i' "$root/target/release/deps/build-alias") == "$original_inode" &&
     $(stat -c '%u:%g:%a:%s:%y' "$root/target/release/deps/build-alias") == "$original_metadata" ]] ||
    fail 'tamper reached the original Cargo inode'
  jeryu_install_finish 0
  original_pair
  [[ $(<"$root/target/release/deps/build-alias") == 'synthetic binary' ]] ||
    fail 'restoration changed the Cargo alias'
)
(
  new_fixture failed-test
  begin
  jeryu_install_change artifact-tamper
  jeryu_install_change receipt-remove
  jeryu_install_change fixture-create
  if jeryu_install_finish 19; then fail 'failed test became success'; fi
  original_pair
  recovery_pair
  [[ ! -e $fixture && ! -L $fixture && -d $scratch ]] || fail 'failure did not retain recovery'
  # Explicit retry is only fixture disposal here; real test exits after failed finish.
  jeryu_install_finish 0
)
(
  new_fixture destination-link
  begin
  mv -- "$artifact" "$root/target/release/artifact-held"
  printf 'external sentinel\n' >"$root/sentinel"
  ln -s "$root/sentinel" "$artifact"
  if jeryu_install_finish 0; then fail 'destination symlink accepted'; fi
  recovery_pair
  [[ $(<"$root/sentinel") == 'external sentinel' ]] || fail 'symlink target changed'
  rm -- "$artifact"
  mv -- "$root/target/release/artifact-held" "$artifact"
  jeryu_install_expected[artifact]=$(jeryu_install_file_state "$artifact")
  jeryu_install_finish 0
)
(
  new_fixture destination-hardlink
  begin
  ln "$artifact" "$root/external-alias"
  reject jeryu_install_change artifact-tamper
  if jeryu_install_finish 0; then fail 'new artifact hardlink accepted'; fi
  recovery_pair
  [[ $(<"$root/external-alias") == 'synthetic binary' ]] || fail 'hardlinked inode changed'
  rm -- "$root/external-alias"
  jeryu_install_expected[artifact]=$(jeryu_install_file_state "$artifact")
  jeryu_install_finish 0
)
(
  new_fixture backup-link
  begin
  mv -- "$scratch/artifact" "$scratch/artifact-held"
  ln -s "$root/target/release/jeryu" "$scratch/artifact"
  if jeryu_install_finish 0; then fail 'backup symlink accepted'; fi
  original_pair
  rm -- "$scratch/artifact"
  mv -- "$scratch/artifact-held" "$scratch/artifact"
  jeryu_install_backups[artifact]=$(jeryu_install_file_state "$scratch/artifact")
  jeryu_install_finish 0
)
(
  new_fixture root-alias
  begin
  mv -- "$root" "$root-held"
  mkdir "$root-other"
  ln -s "$root-other" "$root"
  if jeryu_install_finish 0; then fail 'root alias accepted'; fi
  [[ -f "$root-held/tmp/${scratch##*/}/artifact" ]] || fail 'aliased-root recovery lost'
  rm -- "$root"
  mv -- "$root-held" "$root"
  jeryu_install_finish 0
)
(
  new_fixture release-replaced
  begin
  mv -- "$root/target/release" "$root/target/release-held"
  mkdir "$root/target/release"
  if jeryu_install_finish 0; then fail 'replacement release directory accepted'; fi
  recovery_pair
  rmdir "$root/target/release"
  mv -- "$root/target/release-held" "$root/target/release"
  jeryu_install_finish 0
)
(
  new_fixture custody-mode
  begin
  chmod 0750 "$root"
  if jeryu_install_finish 0; then fail 'changed directory mode accepted'; fi
  recovery_pair
  chmod 0700 "$root"
  jeryu_install_finish 0
)
(
  new_fixture scratch-links
  begin
  ln "$scratch/artifact" "$root/recovery-alias"
  if jeryu_install_finish 0; then fail 'recovery hardlink accepted'; fi
  rm -- "$root/recovery-alias"
  jeryu_install_backups[artifact]=$(jeryu_install_file_state "$scratch/artifact")
  printf 'sentinel\n' >"$root/sentinel"
  ln -s "$root/sentinel" "$scratch/escape"
  if jeryu_install_finish 0; then fail 'scratch escape accepted'; fi
  [[ $(<"$root/sentinel") == sentinel ]] || fail 'scratch escape changed target'
  rm -- "$scratch/escape"
  jeryu_install_finish 0
)
(
  new_fixture restore-failure
  begin
  jeryu_install_change artifact-tamper
  jeryu_install_change receipt-corrupt
  mv() {
    if [[ ${*: -1} == */jeryu.source-build ]]; then return 73; fi
    command mv "$@"
  }
  if jeryu_install_finish 0; then fail 'receipt restore failure accepted'; fi
  [[ $(<"$artifact") == 'synthetic binary' && -f $jeryu_install_stage ]] ||
    fail 'restore failure lost the promoted binary or held receipt stage'
  recovery_pair
  [[ $(<"$jeryu_install_stage") == 'synthetic receipt' ]] || fail 'receipt stage bytes changed'
  unset -f mv
  # The test knows this exact single-link stage; remove it before a manual fixture retry.
  [[ $(jeryu_install_file_state "$jeryu_install_stage") != absent ]] || fail 'stage custody lost'
  rm -- "$jeryu_install_stage"
  jeryu_install_stage=''
  jeryu_install_finish 0
)
(
  new_fixture capture-failure
  cp() {
    if [[ ${*: -1} == */receipt ]]; then
      printf 'partial\n' >"${*: -1}"
      return 74
    fi
    command cp "$@"
  }
  if jeryu_install_begin "$root"; then fail 'partial recovery capture accepted'; fi
  unset -f cp
  if jeryu_install_finish 1; then fail 'partial capture became success'; fi
  original_pair
  [[ $(<"$scratch/artifact") == 'synthetic binary' && $(<"$scratch/receipt") == partial ]] ||
    fail 'partial recovery was discarded'
  jeryu_install_private_tree
  jeryu_remove_test_scratch
)
(
  new_fixture initial-receipt-hardlink
  ln "$root/target/release/jeryu.source-build" "$root/receipt-alias"
  reject jeryu_install_begin "$root"
  original_pair
)
(
  new_fixture scratch-replaced
  begin
  mv -- "$scratch" "$scratch-held"
  mkdir "$scratch"
  if jeryu_install_finish 0; then fail 'replacement scratch root accepted'; fi
  [[ $(<"$scratch-held/artifact") == 'synthetic binary' ]] || fail 'replacement lost recovery'
  rmdir "$scratch"
  mv -- "$scratch-held" "$scratch"
  jeryu_install_finish 0
)
(
  new_fixture target-alias
  begin
  mv -- "$root/target" "$root/target-held"
  mkdir "$root/other-target"
  ln -s "$root/other-target" "$root/target"
  if jeryu_install_finish 0; then fail 'target alias accepted'; fi
  recovery_pair
  [[ -f "$root/target-held/release/jeryu" ]] || fail 'target alias changed held artifact'
  rm -- "$root/target"
  mv -- "$root/target-held" "$root/target"
  jeryu_install_finish 0
)
(
  new_fixture backup-bytes
  begin
  mv -- "$scratch/artifact" "$scratch/artifact-held"
  printf 'unexpected recovery bytes\n' >"$scratch/artifact"
  if jeryu_install_finish 0; then fail 'changed backup bytes accepted'; fi
  original_pair
  rm -- "$scratch/artifact"
  mv -- "$scratch/artifact-held" "$scratch/artifact"
  jeryu_install_backups[artifact]=$(jeryu_install_file_state "$scratch/artifact")
  jeryu_install_finish 0
)
(
  new_fixture stage-alias
  begin
  jeryu_install_change artifact-tamper
  printf 'external sentinel\n' >"$root/sentinel"
  cp() {
    if [[ ${*: -1} == /proc/self/fd/* ]]; then
      mv -- "$jeryu_install_stage" "$jeryu_install_stage-held"
      ln -s "$root/sentinel" "$jeryu_install_stage"
    fi
    command cp "$@"
  }
  if jeryu_install_finish 0; then fail 'changed stage path accepted'; fi
  [[ $(<"$root/sentinel") == 'external sentinel' && -L $jeryu_install_stage ]] ||
    fail 'stage alias redirected a write'
  recovery_pair
  unset -f cp
  rm -- "$jeryu_install_stage"
  [[ $(jeryu_install_file_state "$jeryu_install_stage-held") != absent ]] || fail 'held stage lost'
  rm -- "$jeryu_install_stage-held"
  jeryu_install_stage=''
  jeryu_install_finish 0
)
reject jeryu_install_no_mounts /proc
printf 'source-install transaction hostiles passed (18 scenarios)\n'
