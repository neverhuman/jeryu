#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
# shellcheck source=/dev/null
source "${repo_root}/tests/scratch.sh"
scratch="$(mktemp -d /tmp/jeryu-finder-scratch-hostiles.XXXXXX)"
jeryu_record_test_scratch "$scratch"
cleanup() {
  local status=$?
  jeryu_remove_test_scratch || {
    printf 'retaining changed scratch-helper fixture: %s\n' "$scratch" >&2
    status=1
  }
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

fail() { printf 'scratch hostile test failed: %s\n' "$*" >&2; exit 1; }

(
  directory="$scratch/internal"
  mkdir -p "$directory/data"
  printf 'owned\n' >"$directory/data/value"
  ln -s "$directory/data" "$directory/link"
  # A line break in a link name must remain one inspected path.
  ln -s "$directory/data" "$directory/"$'line\nbreak'
  jeryu_record_test_scratch "$directory"
  jeryu_remove_test_scratch || fail 'internal links were not safely removed'
  [[ ! -e $directory ]] || fail 'ordinary scratch remained'
)
(
  directory="$scratch/external"
  mkdir "$directory"
  printf 'sentinel\n' >"$scratch/sentinel"
  ln -s "$scratch/sentinel" "$directory/escape"
  jeryu_record_test_scratch "$directory"
  if jeryu_remove_test_scratch; then fail 'external link was accepted'; fi
  [[ -d $directory && -L $directory/escape &&
     $(cat "$scratch/sentinel") == sentinel ]] || fail 'refusal changed the fixture'
)
(
  directory="$scratch/replaced"
  mkdir "$directory"
  jeryu_record_test_scratch "$directory"
  mv -- "$directory" "$scratch/original"
  mkdir "$directory"
  if jeryu_remove_test_scratch; then fail 'replacement directory was accepted'; fi
  [[ -d $directory && -d $scratch/original ]] || fail 'replacement refusal removed a directory'
)
(
  mkdir "$scratch/physical"
  ln -s "$scratch/physical" "$scratch/alias"
  if jeryu_record_test_scratch "$scratch/alias"; then fail 'root alias was accepted'; fi
)
printf 'scratch hostiles ok\n'
