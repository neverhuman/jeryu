#!/usr/bin/env bash
set -euo pipefail
mode="${1:-smoke}"
if ! command -v git >/dev/null 2>&1; then
  echo "git oracle skipped: git binary not found" >&2
  exit 0
fi
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
deploy_root="$(cd "$script_dir/../.." && pwd -P)"
split_root="$(cd "$deploy_root/.." && pwd -P)"
core_root="${JERYU_CORE_ROOT:-$split_root/jeryu-core}"
if [[ ! -f "$core_root/Cargo.toml" ]]; then
  echo "git oracle failed: jeryu-core Cargo.toml not found at $core_root" >&2
  exit 1
fi
cargo_core_run() {
  local package="$1"
  shift
  cargo run --manifest-path "$core_root/Cargo.toml" -q -p "$package" -- "$@"
}
repo_root="$(mktemp -d)"
trap 'rm -rf "$repo_root"' EXIT
mkdir -p "$repo_root/repos"
cargo_core_run jeryu-gitd init-repo --root "$repo_root/repos" oracle demo >/dev/null
work="$repo_root/work"
git init "$work" >/dev/null
git -C "$work" config user.email oracle@example.invalid
git -C "$work" config user.name "Git Oracle"
printf 'oracle\n' > "$work/README.md"
git -C "$work" add README.md
git -C "$work" commit -m 'oracle seed' >/dev/null
git -C "$work" branch -M main
git -C "$work" remote add origin "$repo_root/repos/oracle/demo.git"
seed_oid="$(git -C "$work" rev-parse HEAD)"
git --git-dir="$repo_root/repos/oracle/demo.git" fetch --quiet "$work" HEAD
git --git-dir="$repo_root/repos/oracle/demo.git" update-ref refs/heads/main "$seed_oid" 0000000000000000000000000000000000000000
git --git-dir="$repo_root/repos/oracle/demo.git" symbolic-ref HEAD refs/heads/main
git clone --branch main "$repo_root/repos/oracle/demo.git" "$repo_root/clone" >/dev/null
cmp "$work/README.md" "$repo_root/clone/README.md"
git -C "$repo_root/repos/oracle/demo.git" fsck --strict >/dev/null
cargo_core_run jeryu-mirror-cli import-local --data-dir "$repo_root/data" --owner local "$work" > "$repo_root/import.json"
imported_repo="$repo_root/data/git/local/work.git"
test -f "$imported_repo/jeryu/repo-id"
git clone --branch main "$imported_repo" "$repo_root/import-clone" >/dev/null
cmp "$work/README.md" "$repo_root/import-clone/README.md"
printf 'imported-fetch\n' >> "$work/README.md"
git -C "$work" commit -am 'imported fetch' >/dev/null
cargo_core_run jeryu-mirror-cli import-local --data-dir "$repo_root/data" --owner local "$work" > "$repo_root/import-fetch.json"
git -C "$repo_root/import-clone" fetch origin main >/dev/null
git -C "$repo_root/import-clone" merge --ff-only FETCH_HEAD >/dev/null
cmp "$work/README.md" "$repo_root/import-clone/README.md"
if [[ "$mode" == "full" ]]; then
  echo "git oracle full: smoke passed; extend this harness with the full P0 command matrix"
else
  echo "git oracle smoke: passed"
fi
