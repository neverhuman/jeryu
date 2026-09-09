#!/usr/bin/env bash
# Produce fresh, complete cargo-mutants evidence; retain attempts for diagnosis.
set -euo pipefail
[[ $# == 5 ]] || { echo 'usage: coverage-mutants.sh OUTPUT PACKAGE TIMEOUT BUILD_TIMEOUT JOBS' >&2; exit 2; }
output=$1 package=$2 timeout=$3 build_timeout=$4 jobs=$5
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
version=$(cargo mutants --version)
[[ $version == 'cargo-mutants 25.3.1' ]] || {
  echo 'coverage mutation producer requires cargo-mutants 25.3.1' >&2; exit 1;
}
[[ $(realpath -m -- "$output") == "$(realpath -m -s -- "$output")" ]] || exit 1
mkdir -p -- "$output"
output=$(realpath -e -- "$output")
attempt=$(umask 077; mktemp -d "$output/run.XXXXXXXX")
identity=$(stat -c '%d:%i:%u:%g:%a' -- "$attempt")
# A unique empty parent prevents cargo-mutants from rotating/deleting previous
# evidence or accepting a preceding invocation's partially written outcomes.
printf '[coverage] mutation attempt retained at %s\n' "$attempt"
rc=0
cargo mutants -p "$package" --cargo-arg=--locked --output "$attempt" \
  --timeout "$timeout" --build-timeout "$build_timeout" --jobs "$jobs" --no-times || rc=$?
# v25.3.1: 2 is a completed run with missed mutants. 3 (timeouts), 4 (baseline
# failure), usage/internal errors, and interrupted processes cannot qualify.
[[ $rc == 0 || $rc == 2 ]] || { echo "mutation producer did not complete: exit $rc" >&2; exit 1; }
[[ ! -L $attempt && $(realpath -e -- "$attempt") == "$attempt" &&
   $(stat -c '%d:%i:%u:%g:%a' -- "$attempt") == "$identity" ]] || exit 1
produced=$attempt/mutants.out
[[ -d $produced && ! -L $produced && $(realpath -e -- "$produced") == "$produced" ]] || exit 1
for file in outcomes.json mutants.json lock.json; do
  [[ -f $produced/$file && -s $produced/$file && ! -L $produced/$file &&
     $(stat -c '%h:%u' -- "$produced/$file") == "1:$(id -u)" ]] || {
    echo "missing regular mutation evidence: $file" >&2; exit 1;
  }
done
jq -e -s --arg package "$package" --argjson rc "$rc" \
  --slurpfile selected "$produced/mutants.json" --slurpfile locks "$produced/lock.json" \
  -f "$here/coverage-mutants.jq" "$produced/outcomes.json" >/dev/null || {
  echo 'mutation evidence is incomplete or inconsistent' >&2; exit 1;
}
# Publish only validated evidence to the existing audit paths. Never follow a
# compatibility symlink or overwrite a multiply linked existing evidence file.
[[ ! -L $output/mutants.out ]] || exit 1
mkdir -p -- "$output/mutants.out"
for destination in "$output/mutants.out/outcomes.json" "$output/outcomes.json"; do
  if [[ -e $destination || -L $destination ]]; then
    [[ -f $destination && ! -L $destination && $(stat -c '%h' -- "$destination") == 1 ]] || exit 1
  fi
  staging=$(umask 077; mktemp "$(dirname -- "$destination")/.outcomes.XXXXXXXX")
  cat -- "$produced/outcomes.json" >"$staging"
  cmp -s -- "$produced/outcomes.json" "$staging"
  mv -T -- "$staging" "$destination"
done
printf '[coverage] complete mutation evidence: %s (producer exit %s)\n' "$produced" "$rc"
