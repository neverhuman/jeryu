#!/usr/bin/env bash
# Synthetic orchestration only; never invokes Cargo, export tooling, or real clones.
set -euo pipefail
root=${2:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)}
entrypoint=${1:-$root/scripts/test-split-exports.sh}
# shellcheck source=/dev/null
source "$root/tests/scratch.sh"
umask 077
temporary=$(mktemp -d -t jeryu-split-export-tests.XXXXXXXX)
jeryu_record_test_scratch "$temporary"
cleanup() {
  local result=$?
  jeryu_remove_test_scratch || result=1
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$temporary/bin"
cat > "$temporary/bin/git" <<'GIT'
#!/usr/bin/env bash
set -euo pipefail
while [[ ${1:-} == -C || ${1:-} == -c ]]; do
  if [[ $1 == -C ]]; then cd -- "$2"; fi
  shift 2
done
case $1 in
  rev-parse)
    if [[ $PWD == "$SPLIT_TEST_ROOT" ]]; then
      if [[ $2 == HEAD ]]; then
        [[ $SPLIT_TEST_MODE != source-head-read-fail || ! -e $SPLIT_TEST_ROOT/ran ]] || exit 128
        if [[ $SPLIT_TEST_MODE == source-head-change && -e $SPLIT_TEST_ROOT/ran ]]; then printf '%040d\n' 9; else printf '%040d\n' 1; fi
      else
        if [[ $SPLIT_TEST_MODE == source-tree-change && -e $SPLIT_TEST_ROOT/ran ]]; then printf '%040d\n' 9; else printf '%040d\n' 2; fi
      fi
    elif [[ $2 == HEAD ]]; then
      if [[ $SPLIT_TEST_MODE == split-head-change && -e ran ]]; then printf '%040d\n' 9; else printf '%040d\n' 3; fi
    else
      printf '%040d\n' 4
    fi
    ;;
  status)
    if [[ $PWD == "$SPLIT_TEST_ROOT" ]]; then
      [[ $SPLIT_TEST_MODE != source-status-fail ]] || exit 128
      [[ $SPLIT_TEST_MODE != source-final-status-fail || ! -e ran ]] || exit 128
      if [[ $SPLIT_TEST_MODE == source-dirty || ( $SPLIT_TEST_MODE == source-untracked && -e ran ) ]]; then printf '?? synthetic-untracked\n'; fi
    else
      [[ $SPLIT_TEST_MODE != split-status-fail || ! -e ran ]] || exit 128
      if [[ $SPLIT_TEST_MODE == split-untracked && -e ran ]]; then printf '?? synthetic-untracked\n'; fi
      if [[ $SPLIT_TEST_MODE == split-initial-dirty ]]; then printf ' M synthetic-tracked\n'; fi
    fi
    ;;
  ls-tree)
    case $SPLIT_TEST_MODE in
      inventory-fail) exit 128 ;;
      inventory-empty) exit 0 ;;
      inventory-duplicate) printf 'jeryu-cache\njeryu-cache\n' ;;
      inventory-malformed) printf 'jeryu-cache\n../outside\n' ;;
      *) cat "$SPLIT_TEST_ROOT/components.txt" ;;
    esac
    ;;
  commit-tree) printf '%040d\n' 3 ;;
  clone)
    checkout=${!#}
    mkdir -p "$checkout/scripts"
    install -m 0700 "$SPLIT_TEST_ROOT/ordinary.sh" "$checkout/scripts/split-ci.sh"
    printf '%s\n' "$checkout" >> "$SPLIT_TEST_ROOT/checkouts"
    ;;
  fetch|checkout) ;;
  *) printf 'unexpected synthetic Git invocation\n' >&2; exit 97 ;;
esac
GIT
cat > "$temporary/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
set -euo pipefail
[[ $* == 'build --locked -p jeryu-split-tool --bin jeryu-split' ]] || exit 97
printf 'build\n' >> "$SPLIT_TEST_ROOT/events"
CARGO
cat > "$temporary/bin/find" <<'FIND'
#!/usr/bin/env bash
set -euo pipefail
if [[ ${SPLIT_TEST_MODE:-} == cleanup-find-fail && $1 == */jeryu-split-proof.* ]]; then exit 73; fi
exec /usr/bin/find "$@"
FIND
chmod 0700 "$temporary/bin/git" "$temporary/bin/cargo" "$temporary/bin/find"
new_case() {
  local label=$1
  fixture="$temporary/$label"
  mkdir -p "$fixture/scripts" "$fixture/tests" "$fixture/target/debug" "$fixture/tmp"
  # Only the entrypoint and shared guard under test are installed into this
  # synthetic fixture. There is no copied repository or real Git state.
  install -m 0700 "$entrypoint" "$fixture/scripts/test-split-exports.sh"
  install -m 0600 "$root/tests/scratch.sh" "$fixture/tests/scratch.sh"
  printf '%s\n' jeryu-cache jeryu-ci-runner jeryu-core jeryu-deploy jeryu-intelligence \
    jeryu-jira jeryu-release-ops jeryu-tool-finder jeryu-tool jeryu-web > "$fixture/components.txt"
  : > "$fixture/events"
  cat > "$fixture/target/debug/jeryu-split" <<'EXPORT'
#!/usr/bin/env bash
set -euo pipefail
[[ $# == 6 && $1 == export-tree && $2 == --component && $4 == --source && $6 == --resolve-lock ]] || exit 97
component=$3
printf 'export %s\n' "$component" >> "$SPLIT_TEST_ROOT/events"
counter="$SPLIT_TEST_ROOT/export-$component"
count=0
if [[ -e $counter ]]; then read -r count < "$counter"; fi
count=$((count + 1))
printf '%s\n' "$count" > "$counter"
if [[ $component == jeryu-cache && $SPLIT_TEST_MODE == export-fail && $count == 1 ]]; then exit 23; fi
tree=$(printf '%040d' 4)
if [[ $component == jeryu-cache && $SPLIT_TEST_MODE == nondeterministic && $count == 2 ]]; then tree=$(printf '%040d' 9); fi
jq -n --arg component "$component" --arg source "$5" --arg tree "$tree" \
  '{component:$component,source_commit:$source,tree:$tree,lock_regeneration_required:false,publication_qualified:false}'
EXPORT
  cat > "$fixture/ordinary.sh" <<'ORDINARY'
#!/usr/bin/env bash
set -euo pipefail
[[ $* == ordinary ]] || exit 97
component=${PWD##*/}
printf 'ordinary %s\n' "$component" >> "$SPLIT_TEST_ROOT/events"
: > ran
: > "$SPLIT_TEST_ROOT/ran"
case $SPLIT_TEST_MODE in
  ordinary-fail|split-untracked|split-status-fail|split-head-change|source-*)
    printf 'unique synthetic output before failure\n' > unique-output.txt ;;
esac
if [[ $SPLIT_TEST_MODE == ordinary-fail && $component == jeryu-cache ]]; then exit 29; fi
if [[ $SPLIT_TEST_MODE == cleanup-external-link ]]; then ln -s "$SPLIT_TEST_ROOT/target" external-link; fi
ORDINARY
  chmod 0700 "$fixture/target/debug/jeryu-split" "$fixture/ordinary.sh"
}
run_case() {
  local mode=$1
  shift
  env PATH="$temporary/bin:/usr/bin:/bin" TMPDIR="$fixture/tmp" CARGO_TARGET_DIR="$fixture/target" \
    SPLIT_TEST_ROOT="$fixture" SPLIT_TEST_MODE="$mode" \
    bash "$fixture/scripts/test-split-exports.sh" "$@"
}
fail() { printf 'split export hostile test failed: %s\n' "$1" >&2; exit 1; }
assert_count() {
  local expected=$1 pattern=$2
  [[ $(awk -v pattern="$pattern" '$0 ~ pattern {n++} END {print n+0}' "$fixture/events") == "$expected" ]] || fail "$pattern count"
}
assert_removed() {
  local checkout
  if [[ -f $fixture/checkouts ]]; then
    while IFS= read -r checkout; do [[ ! -e ${checkout%/*} && ! -L ${checkout%/*} ]] || fail 'scratch retained after clean cleanup'; done < "$fixture/checkouts"
  fi
}
passed=0
new_case repeat
run_case pass > "$fixture/first.log" 2>&1 || fail 'controlled complete run'
assert_count 20 '^export '
assert_count 10 '^ordinary '
assert_removed
commit=$(printf '%040d' 1)
attempts=("$fixture/target/split-evidence/$commit"/attempt.*)
[[ ${#attempts[@]} == 1 && -d ${attempts[0]} ]] || fail 'first attempt directory'
first_attempt=${attempts[0]}
find "$first_attempt" -type f -exec sha256sum {} + | sort > "$fixture/first.sha256"
run_case pass > "$fixture/second.log" 2>&1 || fail 'controlled repeat run'
attempts=("$fixture/target/split-evidence/$commit"/attempt.*)
[[ ${#attempts[@]} == 2 && ${attempts[0]} != "${attempts[1]}" ]] || fail 'repeat overwrote evidence'
sha256sum --check --status "$fixture/first.sha256" || fail 'first attempt bytes changed'
assert_count 40 '^export '
assert_count 20 '^ordinary '
assert_removed
jq -e '.source_state == "clean" and .publication_qualified == false' "$first_attempt/source.json" >/dev/null
passed=$((passed + 1))

new_case one-component
run_case pass --component jeryu-tool > "$fixture/result.log" 2>&1 || fail 'selected component'
assert_count 2 '^export jeryu-tool$'
assert_count 1 '^ordinary jeryu-tool$'
assert_removed
passed=$((passed + 1))

for mode in source-status-fail source-dirty inventory-fail inventory-empty inventory-duplicate inventory-malformed \
  export-fail nondeterministic ordinary-fail split-initial-dirty split-untracked split-status-fail split-head-change \
  source-head-change source-head-read-fail source-tree-change source-untracked source-final-status-fail \
  cleanup-find-fail cleanup-external-link; do
  new_case "$mode"
  if run_case "$mode" > "$fixture/result.log" 2>&1; then fail "$mode accepted"; fi
  if rg -q '^Split export checks passed' "$fixture/result.log"; then fail "$mode printed aggregate success"; fi
  case $mode in
    source-status-fail|source-dirty|inventory-*) assert_count 0 '^build$' ;;
    export-fail) assert_count 19 '^export '; assert_count 9 '^ordinary '; assert_count 1 '^ordinary jeryu-web$' ;;
    nondeterministic) assert_count 20 '^export '; assert_count 9 '^ordinary '; assert_count 1 '^ordinary jeryu-web$' ;;
    ordinary-fail) assert_count 20 '^export '; assert_count 10 '^ordinary '; assert_count 1 '^ordinary jeryu-web$' ;;
  esac
  if [[ -f $fixture/checkouts ]]; then
    while IFS= read -r checkout; do
      [[ -d $checkout && ! -L $checkout && -d ${checkout%/*} && ! -L ${checkout%/*} ]] ||
        fail 'failed verification discarded a split checkout'
    done < "$fixture/checkouts"
    read -r checkout < "$fixture/checkouts"
    case $mode in
      ordinary-fail|split-untracked|split-status-fail|split-head-change|source-*)
        cmp "$checkout/unique-output.txt" <(printf 'unique synthetic output before failure\n') ||
          fail 'unique failed-command output was not retained' ;;
    esac
  fi
  passed=$((passed + 1))
done

new_case unknown-component
if run_case pass --component jeryu-unknown > "$fixture/result.log" 2>&1; then fail 'unknown component accepted'; fi
assert_count 0 '^build$'
passed=$((passed + 1))
new_case evidence-link
mkdir "$fixture/elsewhere"
ln -s "$fixture/elsewhere" "$fixture/target/split-evidence"
if run_case pass > "$fixture/result.log" 2>&1; then fail 'linked evidence parent accepted'; fi
[[ -d $fixture/elsewhere && -L $fixture/target/split-evidence ]] || fail 'evidence link target changed'
assert_count 0 '^export '
passed=$((passed + 1))
# Every intentionally retained child/link belongs to this outer synthetic root;
# the shared guard inspects all links before removing the complete test fixture.
jeryu_remove_test_scratch
trap - EXIT
printf 'Split export orchestration: %s synthetic cases passed; no real split qualification.\n' "$passed"

# The shared local-source transport is exercised separately from orchestration.
bash "$root/tests/split-local-hostiles.sh" "$root"
