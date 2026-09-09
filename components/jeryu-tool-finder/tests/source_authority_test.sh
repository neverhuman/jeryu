#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
authority="$repo_root/ops/ci/source-authority.sh"
# shellcheck source=/dev/null
source "$repo_root/tests/scratch.sh"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-tool-finder-source-authority.XXXXXX")"
jeryu_record_test_scratch "$test_root"
fixture="$test_root/fixture"
foreign="$test_root/foreign"
stderr_log="$test_root/stderr.log"

cleanup() {
  local status=$?
  jeryu_remove_test_scratch || status=1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

fail() {
  printf 'source-authority hostile test failed: %s\n' "$1" >&2
  sed -n '1,12p' "$stderr_log" >&2
  exit 1
}

init_repo() {
  local path="$1"
  mkdir -p -- "$path/src" "$path/contracts"
  /usr/bin/git -C "$path" init -q
  /usr/bin/git -C "$path" config user.name fixture
  /usr/bin/git -C "$path" config user.email fixture@example.invalid
  printf '[package]\nname="jeryu-tool-finder"\nversion="5.1.0"\n' > "$path/Cargo.toml"
  printf '# lock\n' > "$path/Cargo.lock"
  printf '[toolchain]\nchannel="1.95.0"\n' > "$path/rust-toolchain.toml"
  printf 'jeryu-tool-finder-v5.1.0-split.0\n' > "$path/VERSION"
  printf 'fn main() {}\n' > "$path/src/main.rs"
  printf 'help\n' > "$path/contracts/cli-help.txt"
  printf 'target/\ndossiers/\n.jankurai/\n' > "$path/.gitignore"
  /usr/bin/git -C "$path" add .
  /usr/bin/git -C "$path" commit -q -m fixture
}

snapshot() {
  (
    export JERYU_MONOREPO_CANDIDATE=0
    repo_root="$fixture"
    # shellcheck source=../ops/ci/source-authority.sh
    source "$authority"
    jeryu_source_snapshot
    printf '%s %s %s\n' \
      "$JERYU_SOURCE_HEAD" "$JERYU_SOURCE_TREE" "$JERYU_SOURCE_INPUTS_SHA256"
  ) 2> "$stderr_log"
}

expect_rejected() {
  if snapshot >/dev/null; then
    fail "$1"
  fi
}

init_repo "$fixture"
init_repo "$foreign"
baseline="$(snapshot)" || fail 'clean physical fixture was rejected'
fixture_head="${baseline%% *}"

# A different commit with the same tree proves ambient authority cannot select
# the foreign repository while the caller remains in the canonical cwd.
/usr/bin/git -C "$foreign" commit -q --allow-empty -m same-tree-foreign
foreign_head="$(/usr/bin/git -C "$foreign" rev-parse HEAD)"
[[ "$foreign_head" != "$fixture_head" ]] || fail 'foreign commit did not differ'
ambient="$(GIT_DIR="$foreign/.git" GIT_WORK_TREE="$fixture" \
  GIT_INDEX_FILE="$foreign/.git/index" GIT_COMMON_DIR="$foreign/.git" \
  GIT_OBJECT_DIRECTORY="$foreign/.git/objects" GIT_NAMESPACE=foreign \
  GIT_REPLACE_REF_BASE=refs/foreign-replace snapshot)" ||
  fail 'ambient Git variables were not scrubbed'
[[ "${ambient%% *}" == "$fixture_head" ]] || fail 'ambient Git selected the foreign commit'

for hostile in \
  "GIT_DIR=$foreign/.git" \
  "GIT_WORK_TREE=$foreign" \
  "GIT_INDEX_FILE=$foreign/.git/index" \
  "GIT_COMMON_DIR=$foreign/.git" \
  "GIT_OBJECT_DIRECTORY=$foreign/.git/objects" \
  "GIT_ALTERNATE_OBJECT_DIRECTORIES=$foreign/.git/objects" \
  'GIT_NAMESPACE=foreign' \
  'GIT_REPLACE_REF_BASE=refs/foreign-replace' \
  'GIT_CONFIG_GLOBAL=/dev/null' \
  'GIT_CONFIG_SYSTEM=/dev/null'; do
  ambient="$(export "${hostile?}"; snapshot)" ||
    fail "isolated ambient variable was not scrubbed: ${hostile%%=*}"
  [[ "$ambient" == "$baseline" ]] ||
    fail "isolated ambient variable changed authority: ${hostile%%=*}"
done
ambient="$(GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.worktree \
  GIT_CONFIG_VALUE_0="$foreign" snapshot)" || fail 'dynamic Git config was not scrubbed'
[[ "$ambient" == "$baseline" ]] || fail 'dynamic Git config changed source authority'

/usr/bin/git -C "$fixture" update-index --assume-unchanged Cargo.toml
expect_rejected 'assume-unchanged input passed'
/usr/bin/git -C "$fixture" update-index --no-assume-unchanged Cargo.toml
/usr/bin/git -C "$fixture" update-index --skip-worktree Cargo.toml
expect_rejected 'skip-worktree input passed'
/usr/bin/git -C "$fixture" update-index --no-skip-worktree Cargo.toml

mkdir -p -- "$fixture/.cargo"
printf '[build]\ntarget-dir="/tmp/foreign"\n' > "$fixture/.cargo/config.toml"
printf '.cargo/\n' >> "$fixture/.git/info/exclude"
expect_rejected 'ignored Cargo configuration passed'
rm -- "$fixture/.cargo/config.toml"
rmdir -- "$fixture/.cargo"
printf '' > "$fixture/.git/info/exclude"

empty_commit="$(printf 'replacement\n' | /usr/bin/git -C "$fixture" commit-tree \
  "$(/usr/bin/git -C "$fixture" rev-parse 'HEAD^{tree}')" -p HEAD)"
/usr/bin/git -C "$fixture" replace "$fixture_head" "$empty_commit"
expect_rejected 'replacement ref passed'
/usr/bin/git -C "$fixture" replace -d "$fixture_head" >/dev/null

printf '%s\n' "$foreign/.git/objects" > "$fixture/.git/objects/info/alternates"
expect_rejected 'object alternate passed'
rm -- "$fixture/.git/objects/info/alternates"

(
  export JERYU_MONOREPO_CANDIDATE=0
  repo_root="$fixture"
  # shellcheck source=../ops/ci/source-authority.sh
  source "$authority"
  jeryu_source_snapshot
  printf 'changed\n' >> "$fixture/src/main.rs"
  if jeryu_source_verify \
    "$JERYU_SOURCE_HEAD" "$JERYU_SOURCE_TREE" "$JERYU_SOURCE_INPUTS_SHA256"; then
    exit 0
  fi
  exit 37
) 2> "$stderr_log" && fail 'mid-proof source change passed' || status=$?
[[ "$status" -eq 37 ]] || fail 'TOCTOU hostile returned an unexpected status'
/usr/bin/git -C "$fixture" checkout -q -- src/main.rs

[[ "$(snapshot)" == "$baseline" ]] || fail 'restored fixture identity changed'

for lane in ops/ci/score.sh ops/ci/security.sh ops/ci/artifact-support.sh; do
  grep -Fq 'source "$repo_root/ops/ci/source-authority.sh"' "$repo_root/$lane" ||
    fail "$lane does not load canonical source authority"
  grep -Fq 'jeryu_source_snapshot' "$repo_root/$lane" ||
    fail "$lane does not snapshot physical source identity"
  grep -Fq 'jeryu_source_verify' "$repo_root/$lane" ||
    fail "$lane does not close source TOCTOU"
  if grep -Eq '(^|[;&|()[:space:]])git[[:space:]]' "$repo_root/$lane"; then
    fail "$lane contains a raw Git invocation"
  fi
done
printf 'source-authority hostile tests ok\n'
