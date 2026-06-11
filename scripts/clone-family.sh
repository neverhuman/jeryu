#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
default_dest="$(cd "${repo_root}/.." && pwd)"
dest="${1:-$default_dest}"
manifest="${JERYU_SPLIT_MANIFEST:-${repo_root}/repos.manifest.toml}"

[[ -r "$manifest" ]] || { printf 'manifest not readable: %s\n' "$manifest" >&2; exit 1; }
mkdir -p "$dest"

mapfile -t rows < <(
  python3 - "$manifest" <<'PY'
import sys
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
with open(sys.argv[1], "rb") as fh:
    data = tomllib.load(fh)
for repo in data.get("repo", []):
    print("|".join([
        str(repo.get("name", "")),
        str(repo.get("github_slug", "")),
        str(repo.get("profile", "")),
    ]))
PY
)

for row in "${rows[@]}"; do
  IFS='|' read -r name github_slug profile <<<"$row"
  [[ -n "$name" && -n "$github_slug" ]] || continue
  if [[ "$profile" == "public-portal" && "${JERYU_CLONE_PORTAL:-0}" != "1" ]]; then
    continue
  fi
  target="${dest}/${name}"
  remote="https://github.com/${github_slug}.git"
  if [[ -d "${target}/.git" ]]; then
    printf 'updating %s\n' "$target"
    git -C "$target" fetch --prune origin
    branch="$(git -C "$target" symbolic-ref --quiet --short HEAD || printf 'main')"
    git -C "$target" pull --ff-only origin "$branch"
  elif [[ -e "$target" ]]; then
    printf 'refusing to overwrite non-git path: %s\n' "$target" >&2
    exit 1
  else
    printf 'cloning %s -> %s\n' "$remote" "$target"
    git clone "$remote" "$target"
  fi
done
