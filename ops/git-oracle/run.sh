#!/usr/bin/env bash
set -euo pipefail
mode="${1:-smoke}"
if ! command -v git >/dev/null 2>&1; then
  echo "git oracle skipped: git binary not found" >&2
  exit 0
fi
repo_root="$(mktemp -d)"
trap 'rm -rf "$repo_root"' EXIT
mkdir -p "$repo_root/repos"
cargo run -q -p jeryu-gitd -- init-repo --root "$repo_root/repos" oracle demo >/dev/null
work="$repo_root/work"
git init "$work" >/dev/null
git -C "$work" config user.email oracle@example.invalid
git -C "$work" config user.name "Git Oracle"
printf 'oracle\n' > "$work/README.md"
git -C "$work" add README.md
git -C "$work" commit -m 'oracle seed' >/dev/null
git -C "$work" remote add origin "$repo_root/repos/oracle/demo.git"
git -C "$work" push origin HEAD:refs/heads/main >/dev/null
git clone "$repo_root/repos/oracle/demo.git" "$repo_root/clone" >/dev/null
cmp "$work/README.md" "$repo_root/clone/README.md"
git -C "$repo_root/repos/oracle/demo.git" fsck --strict >/dev/null
if [[ "$mode" == "full" ]]; then
  echo "git oracle full: smoke passed; extend this harness with the full P0 command matrix"
else
  echo "git oracle smoke: passed"
fi
