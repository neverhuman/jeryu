#!/usr/bin/env bash
# Tiny real Git source fixtures and synthetic command transport; no clone/Cargo/network.
set -euo pipefail
root=${1:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)}
helper=${2:-$root/scripts/source-build.sh}
entrypoint=${3:-$root/scripts/test-split-exports.sh}
split_ci=${4:-$root/components/jeryu-deploy/crates/jeryu-split-tool/src/split_ci.sh}
[[ $# -le 4 ]] || exit 2
# shellcheck source=/dev/null
source "$root/tests/scratch.sh"
# shellcheck source=/dev/null
source "$helper"
umask 077
temporary=$(mktemp -d -t jeryu-split-local-tests.XXXXXXXX)
jeryu_record_test_scratch "$temporary"
finish() {
  local result=$?
  trap - EXIT
  if (( result == 0 )); then jeryu_remove_test_scratch || result=1; fi
  if (( result != 0 )); then printf 'retained local transport fixtures: %s\n' "$temporary" >&2; fi
  exit "$result"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
fixture=$temporary/source
mkdir -p "$fixture/components/jeryu-cache" "$fixture/components/jeryu-core" "$fixture/components/jeryu-intelligence"
printf 'synthetic source\n' > "$fixture/README.md"
printf 'synthetic component\n' > "$fixture/components/jeryu-cache/Cargo.toml"
for component in jeryu-core jeryu-intelligence; do
  printf 'synthetic component\n' > "$fixture/components/$component/Cargo.toml"
done
split_source_git "$fixture" init --quiet --initial-branch=main --template=
split_source_git "$fixture" add .
split_source_git "$fixture" -c user.name=Fixture -c user.email=fixture@jeryu.invalid \
  -c commit.gpgsign=false commit --quiet -m 'Synthetic source admission'
revision=$(split_source_git "$fixture" rev-parse HEAD)
subtree=$(split_source_git "$fixture" rev-parse HEAD:components/jeryu-cache)
passed=0
expect() {
  local wanted=$1 observed=0
  shift
  "$@" > "$temporary/stdout" 2> "$temporary/stderr" || observed=$?
  [[ $observed == "$wanted" ]] || {
    printf 'local transport test: expected %s, got %s\n' "$wanted" "$observed" >&2
    cat "$temporary/stderr" >&2
    exit 1
  }
  passed=$((passed + 1))
}
expect 0 split_source_snapshot "$fixture" "$revision"
expect 0 split_source_snapshot "$fixture" "$revision" jeryu-cache "$subtree"
expect 1 split_source_snapshot "$fixture" main
expect 1 split_source_snapshot "$fixture" 0000000000000000000000000000000000000000
expect 1 split_source_snapshot "$fixture" "$revision" jeryu-cache 0000000000000000000000000000000000000000
expect 1 split_source_snapshot "$fixture" "$revision" ../cache "$subtree"
expect 1 split_source_snapshot "$fixture/../source" "$revision"
expect 1 split_source_snapshot relative "$revision"
ln -s source "$temporary/alias"
expect 1 split_source_snapshot "$temporary/alias" "$revision"
printf 'dirty\n' >> "$fixture/README.md"
expect 1 split_source_snapshot "$fixture" "$revision"
split_source_git "$fixture" add README.md
expect 1 split_source_snapshot "$fixture" "$revision"
split_source_git "$fixture" reset --quiet --hard "$revision"
printf 'untracked\n' > "$fixture/new.txt"
expect 1 split_source_snapshot "$fixture" "$revision"
rm -- "$fixture/new.txt"
for flag in assume-unchanged skip-worktree; do
  split_source_git "$fixture" update-index "--$flag" README.md
  expect 1 split_source_snapshot "$fixture" "$revision"
  split_source_git "$fixture" update-index "--no-$flag" README.md
done
ln "$fixture/README.md" "$temporary/hardlink"
expect 1 split_source_snapshot "$fixture" "$revision"
rm -- "$temporary/hardlink"
: > "$fixture/.git/index.lock"
expect 1 split_source_snapshot "$fixture" "$revision"
rm -- "$fixture/.git/index.lock"
# Local transport must be self-contained, complete and free of replacement refs.
: > "$fixture/.git/objects/info/alternates"
expect 1 split_source_snapshot "$fixture" "$revision"
rm -- "$fixture/.git/objects/info/alternates"
ln -s "$temporary/missing-objects" "$fixture/.git/objects/info/alternates"
expect 1 split_source_snapshot "$fixture" "$revision"
rm -- "$fixture/.git/objects/info/alternates"
: > "$fixture/.git/objects/info/http-alternates"
expect 1 split_source_snapshot "$fixture" "$revision"
rm -- "$fixture/.git/objects/info/http-alternates"
printf '%s\n' "$revision" > "$fixture/.git/shallow"
[[ $(split_source_git "$fixture" rev-parse --is-shallow-repository) == true ]]
expect 1 split_source_snapshot "$fixture" "$revision"
rm -- "$fixture/.git/shallow"
split_source_git "$fixture" update-ref "refs/replace/$revision" "$revision"
expect 1 split_source_snapshot "$fixture" "$revision"
split_source_git "$fixture" pack-refs --all
expect 1 split_source_snapshot "$fixture" "$revision"
split_source_git "$fixture" update-ref -d "refs/replace/$revision"
# Git status can be clean while a clean filter hides different working bytes.
printf 'README.md filter=synthetic\n' > "$fixture/.gitattributes"
split_source_git "$fixture" config filter.synthetic.clean 'printf "synthetic source\n"'
split_source_git "$fixture" add .gitattributes
split_source_git "$fixture" -c commit.gpgsign=false commit --quiet -m 'Synthetic clean filter'
filtered_revision=$(split_source_git "$fixture" rev-parse HEAD)
printf 'uncommitted bytes hidden by the clean filter\n' > "$fixture/README.md"
split_source_git "$fixture" add README.md
[[ -z $(split_source_git "$fixture" status --porcelain=v1 --untracked-files=all) ]]
expect 1 split_source_snapshot "$fixture" "$filtered_revision"
split_source_git "$fixture" config --unset filter.synthetic.clean
split_source_git "$fixture" reset --quiet --hard "$revision"
# core.filemode=false likewise must not conceal changed executable state.
split_source_git "$fixture" config core.filemode false
chmod 0700 "$fixture/README.md"
[[ -z $(split_source_git "$fixture" status --porcelain=v1 --untracked-files=all) ]]
expect 1 split_source_snapshot "$fixture" "$revision"
chmod 0600 "$fixture/README.md"
split_source_git "$fixture" config core.filemode true
split_source_snapshot "$fixture" "$revision" > /dev/null
# Ambient routing and Git identity cannot select another source or survive into
# the child command. The child's source URL remains the canonical public URL.
cat > "$temporary/observe.sh" <<'OBSERVE'
#!/usr/bin/env bash
set -euo pipefail
[[ ! -v GIT_DIR && ! -v GIT_WORK_TREE && ! -v GIT_CONFIG_KEY_1 &&
   ! -v GIT_SSH_COMMAND && ! -v SSH_AUTH_SOCK && ! -v GIT_REPLACE_REF_BASE &&
   $GIT_CONFIG_GLOBAL == /dev/null && $GIT_CONFIG_SYSTEM == /dev/null &&
   $GIT_CONFIG_NOSYSTEM == 1 && $GIT_NO_REPLACE_OBJECTS == 1 &&
   $GIT_CONFIG_COUNT == 1 && $GIT_CONFIG_KEY_0 == "url.file://$1.insteadOf" &&
   $GIT_CONFIG_VALUE_0 == https://github.com/neverhuman/jeryu.git &&
   $GIT_ALLOW_PROTOCOL == file:https && $CARGO_NET_GIT_FETCH_WITH_CLI == true ]]
printf 'exact closed local transport\n'
OBSERVE
expect 0 env GIT_DIR=/missing GIT_WORK_TREE=/missing GIT_REPLACE_REF_BASE=refs/hostile/ \
  GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=url.hostile.insteadOf GIT_CONFIG_VALUE_0=https://github.com/ \
  GIT_CONFIG_KEY_1=core.sshCommand GIT_CONFIG_VALUE_1=false \
  GIT_SSH_COMMAND=false SSH_AUTH_SOCK=/missing \
  bash -c 'source "$1"; split_source_run "$2" "$3" jeryu-cache "$4" bash "$5" "$2"' \
  split-test "$helper" "$fixture" "$revision" "$subtree" "$temporary/observe.sh"
[[ $(<"$temporary/stdout") == 'exact closed local transport' ]]
expect 23 split_source_run "$fixture" "$revision" jeryu-cache "$subtree" bash -c 'exit 23'
expect 1 split_source_run "$fixture" "$revision" jeryu-cache "$subtree" \
  bash -c 'printf "changed during command\n" >> "$1/README.md"' split-test "$fixture"
[[ $(<"$fixture/README.md") == $'synthetic source\nchanged during command' ]]
split_source_git "$fixture" reset --quiet --hard "$revision"
expect 0 split_source_snapshot "$fixture" "$revision" jeryu-cache "$subtree"
expect 2 bash "$entrypoint" --prepare-local
expect 2 bash "$entrypoint" --prepare-local relative
expect 2 bash "$entrypoint" --prepare-local "$fixture" --prepare-local "$fixture"
expect 2 bash "$entrypoint" --component jeryu-cache --component jeryu-tool
expect 2 bash "$entrypoint" --unrecognized
# Execute the actual generated ordinary entrypoint with a fake Cargo transport.
# The two tiny repositories contain only this fixture and the scripts under test.
mkdir -p "$fixture/scripts" "$temporary/export/scripts" "$temporary/bin"
install -m 0600 "$helper" "$fixture/scripts/source-build.sh"
split_source_git "$fixture" add scripts/source-build.sh
split_source_git "$fixture" -c user.name=Fixture -c user.email=fixture@jeryu.invalid \
  -c commit.gpgsign=false commit --quiet -m 'Synthetic helper projection'
revision=$(split_source_git "$fixture" rev-parse HEAD)
export_fixture=$temporary/export
install -m 0600 "$helper" "$export_fixture/scripts/source-build.sh"
install -m 0600 "$split_ci" "$export_fixture/scripts/split-ci.sh"
printf 'synthetic workspace\n' > "$export_fixture/Cargo.toml"
jq -n --arg source "$revision" --arg tree "$subtree" \
  '{schema_version:"jeryu.split-provenance/v1",component:"jeryu-cache",source_commit:$source,
    original_component_tree:$tree,lock_regeneration_required:false,publication_qualified:false}' > "$export_fixture/.jeryu-source.json"
split_source_git "$export_fixture" init --quiet --initial-branch=main --template=
split_source_git "$export_fixture" add .
split_source_git "$export_fixture" -c user.name=Fixture -c user.email=fixture@jeryu.invalid \
  -c commit.gpgsign=false commit --quiet -m 'Synthetic generated export'
cat > "$temporary/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
set -euo pipefail
if [[ $TRANSPORT_MODE == local ]]; then
  [[ $GIT_CONFIG_COUNT == 1 && $GIT_CONFIG_KEY_0 == "url.file://$EXPECTED_SOURCE.insteadOf" &&
     $GIT_CONFIG_VALUE_0 == https://github.com/neverhuman/jeryu.git &&
     ! -v GIT_DIR && ! -v GIT_CONFIG_KEY_1 && ! -v SSH_AUTH_SOCK ]]
fi
case "$*" in
  'fmt --all -- --check'|'clippy --locked --workspace --all-targets --all-features -- -D warnings'|\
  'test --locked --workspace --all-features'|'build --locked --workspace') ;;
  *) exit 97 ;;
esac
printf '%s\n' "$1" >> "$TRANSPORT_TRACE"
if [[ ${MUTATE_SOURCE:-0} == 1 ]]; then printf 'changed in Cargo fixture\n' >> "$EXPECTED_SOURCE/README.md"; fi
if [[ ${MUTATE_EXPORT:-0} == 1 ]]; then printf 'changed generated source\n' >> Cargo.toml; fi
if [[ $1 == test ]]; then exit "${TEST_EXIT:-0}"; fi
CARGO
chmod 0700 "$temporary/bin/cargo"
export PATH="$temporary/bin:$PATH" TRANSPORT_TRACE="$temporary/cargo.trace" EXPECTED_SOURCE="$fixture"
export TRANSPORT_MODE=local
: > "$TRANSPORT_TRACE"
expect 0 bash "$export_fixture/scripts/split-ci.sh" ordinary --prepare-local "$fixture"
[[ $(<"$TRANSPORT_TRACE") == $'fmt\nclippy\ntest\nbuild' ]]
[[ $(grep -c '^split transport: local-source-preparation ' "$temporary/stderr") == 1 ]]
: > "$TRANSPORT_TRACE"
export TEST_EXIT=23
expect 23 bash "$export_fixture/scripts/split-ci.sh" ordinary --prepare-local "$fixture"
[[ $(<"$TRANSPORT_TRACE") == $'fmt\nclippy\ntest' ]]
unset TEST_EXIT
: > "$TRANSPORT_TRACE"
export MUTATE_SOURCE=1
expect 1 bash "$export_fixture/scripts/split-ci.sh" ordinary --prepare-local "$fixture"
# Whole-block admission still rejects source mutation after all strict commands.
[[ $(<"$TRANSPORT_TRACE") == $'fmt\nclippy\ntest\nbuild' ]]
unset MUTATE_SOURCE
split_source_git "$fixture" reset --quiet --hard "$revision"
: > "$TRANSPORT_TRACE"
expect 2 bash "$export_fixture/scripts/split-ci.sh" ordinary --prepare-local "$fixture" --prepare-local "$fixture"
[[ ! -s $TRANSPORT_TRACE ]]
expect 2 bash "$export_fixture/scripts/split-ci.sh" sandbox --prepare-local "$fixture"
[[ ! -s $TRANSPORT_TRACE ]]
split_source_git "$export_fixture" update-index --assume-unchanged .jeryu-source.json
expect 1 bash "$export_fixture/scripts/split-ci.sh" ordinary --prepare-local "$fixture"
[[ ! -s $TRANSPORT_TRACE ]]
split_source_git "$export_fixture" update-index --no-assume-unchanged .jeryu-source.json
: > "$TRANSPORT_TRACE"
export MUTATE_EXPORT=1
expect 1 bash "$export_fixture/scripts/split-ci.sh" ordinary --prepare-local "$fixture"
[[ $(<"$TRANSPORT_TRACE") == $'fmt\nclippy\ntest\nbuild' ]]
unset MUTATE_EXPORT
export_revision=$(split_source_git "$export_fixture" rev-parse HEAD)
split_source_git "$export_fixture" reset --quiet --hard "$export_revision"
: > "$TRANSPORT_TRACE"
export TRANSPORT_MODE=public
expect 0 bash "$export_fixture/scripts/split-ci.sh" ordinary
[[ $(<"$TRANSPORT_TRACE") == $'fmt\nclippy\ntest\nbuild' ]]

# The other two selected components reuse this exact dispatcher and custody.
export TRANSPORT_MODE=local
for component in jeryu-core jeryu-intelligence; do
  component_tree=$(split_source_git "$fixture" rev-parse "HEAD:components/$component")
  jq --arg component "$component" --arg tree "$component_tree" \
    '.component=$component | .original_component_tree=$tree' \
    "$export_fixture/.jeryu-source.json" > "$temporary/provenance.json"
  install -m 0600 "$temporary/provenance.json" "$export_fixture/.jeryu-source.json"
  split_source_git "$export_fixture" add .jeryu-source.json
  split_source_git "$export_fixture" -c commit.gpgsign=false commit --quiet -m "Synthetic $component selection"
  export_revision=$(split_source_git "$export_fixture" rev-parse HEAD)
  : > "$TRANSPORT_TRACE"
  expect 0 bash "$export_fixture/scripts/split-ci.sh" ordinary --prepare-local "$fixture"
  [[ $(<"$TRANSPORT_TRACE") == $'fmt\nclippy\ntest\nbuild' ]]
  [[ $(grep -c '^split transport: local-source-preparation ' "$temporary/stderr") == 1 ]]
  : > "$TRANSPORT_TRACE"
  export TEST_EXIT=23
  expect 23 bash "$export_fixture/scripts/split-ci.sh" ordinary --prepare-local "$fixture"
  [[ $(<"$TRANSPORT_TRACE") == $'fmt\nclippy\ntest' ]]
  unset TEST_EXIT
  : > "$TRANSPORT_TRACE"
  export MUTATE_SOURCE=1
  expect 1 bash "$export_fixture/scripts/split-ci.sh" ordinary --prepare-local "$fixture"
  [[ $(<"$TRANSPORT_TRACE") == $'fmt\nclippy\ntest\nbuild' ]]
  unset MUTATE_SOURCE
  split_source_git "$fixture" reset --quiet --hard "$revision"
  : > "$TRANSPORT_TRACE"
  export MUTATE_EXPORT=1
  expect 1 bash "$export_fixture/scripts/split-ci.sh" ordinary --prepare-local "$fixture"
  [[ $(<"$TRANSPORT_TRACE") == $'fmt\nclippy\ntest\nbuild' ]]
  unset MUTATE_EXPORT
  split_source_git "$export_fixture" reset --quiet --hard "$export_revision"
done
printf 'Split local source transport: %s synthetic cases passed; no real resolution or build.\n' "$passed"
