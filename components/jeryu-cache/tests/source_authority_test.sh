#!/usr/bin/env bash
# Scanner-free source admission regressions; all synthetic fixtures are retained.
set -euo pipefail
umask 077

component_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
source_library="$component_root/ops/ci/lib.sh"
scratch="$(mktemp -d /tmp/jeryu-cache-source-test.XXXXXXXX)"
[[ ! -L "$scratch" && "$(realpath -e -- "$scratch")" == "$scratch" &&
  "$(stat -c '%u:%a' -- "$scratch")" == "$EUID:700" ]] || exit 1
printf 'Retaining synthetic Cache source fixtures at %s\n' "$scratch" >&2
# No source checkout is copied and no cleanup is attempted, including on success.
cd "$scratch"
fixture_number=0
checks=0

fail() {
  printf 'Cache source authority test failed: %s; retained at %s\n' "$*" "$scratch" >&2
  exit 1
}

fixture_git() {
  /usr/bin/env -i HOME=/nonexistent PATH=/usr/bin:/bin LANG=C LC_ALL=C \
    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_GLOBAL=/dev/null \
    /usr/bin/git -C "$fixture" -c user.name=CacheFixture \
    -c user.email=cache-fixture@example.invalid "$@"
}

new_fixture() {
  local form="$1"
  fixture_number=$((fixture_number + 1))
  fixture="$scratch/fixture-$fixture_number"
  entry="$fixture"
  if [[ "$form" == monorepo ]]; then entry="$fixture/components/jeryu-cache"; fi
  # Git file modes remain normal inside the owner-only outer fixture directory.
  (
    umask 022
    mkdir -- "$fixture"
    mkdir -p -- "$entry/crates/jeryu-cache/src" "$fixture/components/unowned"
    printf 'fixture lock\n' > "$fixture/Cargo.lock"
    printf 'fixture root configuration\n' > "$fixture/root-build-input"
    printf 'pub fn cache_fixture() {}\n' > "$entry/crates/jeryu-cache/src/lib.rs"
    printf 'pub fn other_fixture() {}\n' > "$fixture/components/unowned/lib.rs"
    printf '%s\n' target/ .jankurai/ agent/repo-score.json agent/repo-score.md \
      components/jeryu-cache/agent/repo-score.json \
      components/jeryu-cache/agent/repo-score.md ignored-input/ > "$fixture/.gitignore"
    fixture_git init --quiet --template= --initial-branch=main
    fixture_git add -- .
    fixture_git commit --quiet -m 'synthetic source fixture'
  )
  fixture_head="$(fixture_git rev-parse HEAD)"
}

check_source() {
  /usr/bin/env -i HOME=/nonexistent PATH=/usr/bin:/bin LANG=C LC_ALL=C \
    "$@" /usr/bin/bash --noprofile --norc -c '
      set -euo pipefail
      source "$1"
      jeryu_assert_closed_source_authority "$2" "$3" fixture
    ' cache-source-check "$source_library" "$entry" "$fixture_head"
}

expect_pass() {
  local description="$1"
  checks=$((checks + 1))
  check_source > "$scratch/check-$checks.stdout" 2> "$scratch/check-$checks.stderr" \
    || fail "$description"
}

expect_failure() {
  local description="$1" diagnostic="$2"
  shift 2
  checks=$((checks + 1))
  if check_source "$@" > "$scratch/check-$checks.stdout" 2> "$scratch/check-$checks.stderr"; then
    fail "$description was accepted"
  fi
  grep -Fq -- "$diagnostic" "$scratch/check-$checks.stderr" \
    || fail "$description failed at an unexpected guard"
}

new_fixture standalone
expect_pass 'clean standalone source'
resolved="$(/usr/bin/env -i HOME=/nonexistent PATH=/usr/bin:/bin \
  /usr/bin/bash --noprofile --norc -c '
    set -euo pipefail
    source "$1"
    jeryu_cache_source_root "$2" fixture
  ' cache-root-check "$source_library" "$entry")"
[[ "$resolved" == "$fixture" ]] || fail 'standalone root coordinates'

new_fixture monorepo
expect_pass 'clean component with lockfile only at repository root'
resolved="$(/usr/bin/env -i HOME=/nonexistent PATH=/usr/bin:/bin \
  /usr/bin/bash --noprofile --norc -c '
    set -euo pipefail
    source "$1"
    jeryu_cache_source_root "$2" fixture
  ' cache-root-check "$source_library" "$entry")"
[[ "$resolved" == "$fixture" ]] || fail 'monorepo component root coordinates'
entry="$fixture"
expect_pass 'clean monorepo entered at its repository root'

new_fixture monorepo
mkdir -p -- "$entry/target" "$entry/.jankurai" "$entry/agent" "$fixture/target"
printf 'derived\n' > "$entry/target/output"
printf 'derived\n' > "$entry/.jankurai/report"
printf 'derived\n' > "$entry/agent/repo-score.json"
printf 'derived\n' > "$fixture/target/output"
expect_pass 'exact Cache and root derived outputs'

new_fixture monorepo
printf 'changed\n' >> "$fixture/Cargo.lock"
expect_failure 'modified shared root lock' 'physical bytes do not stably match HEAD'

new_fixture monorepo
fixture_git update-index --assume-unchanged Cargo.lock
printf 'hidden\n' >> "$fixture/Cargo.lock"
expect_failure 'hidden root lock outside component' 'forbidden state flag'

new_fixture monorepo
fixture_git update-index --skip-worktree root-build-input
printf 'hidden\n' >> "$fixture/root-build-input"
expect_failure 'skip-worktree root build input' 'forbidden state flag'

new_fixture monorepo
printf 'changed\n' >> "$fixture/components/unowned/lib.rs"
fixture_git add -- components/unowned/lib.rs
expect_failure 'staged source outside Cache' 'index does not exactly match'

new_fixture monorepo
printf 'untracked\n' > "$fixture/untracked-input"
expect_failure 'untracked root input outside Cache' 'untracked source/build input'

new_fixture monorepo
mkdir -- "$fixture/ignored-input"
printf 'hidden\n' > "$fixture/ignored-input/source.rs"
expect_failure 'ignored root source' 'forbidden ignored source/build input'

new_fixture monorepo
mkdir -- "$entry/ignored-input"
printf 'hidden\n' > "$entry/ignored-input/source.rs"
expect_failure 'ignored component source' 'forbidden ignored source/build input'

new_fixture monorepo
mkdir -- "$fixture/components/unowned/target"
printf 'unowned\n' > "$fixture/components/unowned/target/input.rs"
expect_failure 'unadmitted sibling ignored output' 'forbidden ignored source/build input'

new_fixture standalone
mkdir -- "$fixture/ignored-input"
printf 'hidden\n' > "$fixture/ignored-input/source.rs"
expect_failure 'standalone ignored source remains refused' 'forbidden ignored source/build input'

new_fixture monorepo
ln -- "$fixture/Cargo.lock" "$scratch/root-lock-$fixture_number"
expect_failure 'hard-linked root input outside component' 'single-link regular file'

new_fixture monorepo
chmod 0666 "$fixture/root-build-input"
expect_failure 'world-writable root tracked input' 'physical mode differs'

new_fixture monorepo
ln -s -- "$fixture" "$scratch/root-alias-$fixture_number"
entry="$scratch/root-alias-$fixture_number/components/jeryu-cache"
expect_failure 'symlinked checkout ancestor' 'checkout path traverses a symlink'

new_fixture monorepo
mv -- "$fixture/.git" "$scratch/git-directory-$fixture_number"
printf 'gitdir: %s\n' "$scratch/git-directory-$fixture_number" > "$fixture/.git"
expect_failure 'redirected synthetic Git directory' 'requires its own physical Git directory'

new_fixture monorepo
entry="$fixture/components/unowned"
expect_failure 'arbitrary component cannot select Cache source scope' 'outside the standalone or monorepo Cache source scope'

new_fixture monorepo
fixture_head=0000000000000000000000000000000000000000
expect_failure 'wrong source commit' 'not at the governed HEAD'

new_fixture monorepo
fixture_git update-ref "refs/replace/$fixture_head" "$fixture_head"
expect_failure 'canonical replacement reference' 'canonical Git replacement refs'

new_fixture monorepo
expect_failure 'ambient index authority' 'ambient Git authority variable: GIT_INDEX_FILE' \
  GIT_INDEX_FILE="$scratch/foreign-index"

new_fixture monorepo
mv -- "$fixture/Cargo.lock" "$scratch/symlink-target-$fixture_number"
ln -s -- "$scratch/symlink-target-$fixture_number" "$fixture/Cargo.lock"
fixture_git add -- Cargo.lock
fixture_git commit --quiet -m 'synthetic tracked symlink'
fixture_head="$(fixture_git rev-parse HEAD)"
expect_failure 'tracked symlink in root tree' 'regular stage-zero HEAD file'

new_fixture monorepo
fixture_git update-index --force-remove Cargo.lock
expect_failure 'staged root deletion' 'untracked source/build input'


new_fixture monorepo
monitor="$scratch/fsmonitor-$fixture_number"
marker="$scratch/fsmonitor-ran-$fixture_number"
printf '#!/bin/sh\nprintf invoked > "%s"\nexit 0\n' "$marker" > "$monitor"
chmod 0700 "$monitor"
fixture_git config core.fsmonitor "$monitor"
expect_pass 'local fsmonitor is disabled before every index read'
[[ ! -e "$marker" ]] || fail 'source admission executed a local fsmonitor command'

new_fixture monorepo
filter="$scratch/clean-filter-$fixture_number"
marker="$scratch/filter-ran-$fixture_number"
printf '#!/bin/sh\nprintf invoked > "%s"\ncat\n' "$marker" > "$filter"
chmod 0700 "$filter"
printf 'Cargo.lock filter=fixture\n' > "$fixture/.gitattributes"
chmod 0644 "$fixture/.gitattributes"
fixture_git add -- .gitattributes
fixture_git commit --quiet -m 'synthetic filter attributes'
fixture_head="$(fixture_git rev-parse HEAD)"
fixture_git config filter.fixture.clean "$filter"
fixture_git config filter.fixture.required true
# Force any accidental status refresh to inspect the filter-bound file.
touch -m -d '2001-01-01T00:00:00Z' "$fixture/Cargo.lock"
expect_pass 'source admission does not apply a local clean filter'
[[ ! -e "$marker" ]] || fail 'source admission executed a local clean filter'

printf 'Cache source authority: %s checks passed; retained fixtures: %s\n' "$checks" "$scratch"
