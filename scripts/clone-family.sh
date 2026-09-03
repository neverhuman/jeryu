#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
default_dest="$(cd "${repo_root}/.." && pwd)"
dest=""
manifest="${JERYU_SPLIT_MANIFEST:-${repo_root}/repos.manifest.toml}"
plan=0

usage() {
  printf 'usage: %s [--manifest PATH] [--plan] [DEST]\n' "$0" >&2
}

fail() {
  printf 'clone-family: %s\n' "$1" >&2
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      shift
      [[ $# -gt 0 ]] || { usage; exit 2; }
      manifest="$1"
      ;;
    --plan)
      plan=1
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --*)
      usage
      exit 2
      ;;
    *)
      [[ -z "$dest" ]] || { usage; exit 2; }
      dest="$1"
      ;;
  esac
  shift
done

dest="${dest:-$default_dest}"
[[ "${JERYU_CLONE_PORTAL:-0}" == "0" || "${JERYU_CLONE_PORTAL:-0}" == "1" ]] ||
  fail "JERYU_CLONE_PORTAL must be 0 or 1"
[[ -f "$manifest" && ! -L "$manifest" ]] || fail "manifest must be a readable regular file"

if [[ "$plan" == "1" ]]; then
  dest="$(realpath -m -- "$dest")"
else
  [[ ! -L "$dest" ]] || fail "destination must not be a symlink"
  mkdir -p -- "$dest"
  dest="$(cd "$dest" && pwd -P)"
fi

manifest_rows="$({
  bash "${repo_root}/ops/split/manifest.sh" --manifest "$manifest"
} 2>&1)" || {
  printf '%s\n' "$manifest_rows" >&2
  fail "manifest validation failed"
}
mapfile -t rows <<<"$manifest_rows"
[[ "${#rows[@]}" -gt 0 && -n "${rows[0]}" ]] || fail "manifest contains no repositories"

declare -A seen_names=()
for row in "${rows[@]}"; do
  IFS='|' read -r name _path _github_slug jeryu_slug extra <<<"$row"
  [[ -z "${extra:-}" ]] || fail "manifest row contains an unexpected delimiter"
  [[ "$name" =~ ^[a-z0-9][a-z0-9._-]*$ ]] || fail "manifest contains an invalid repository name"
  [[ "$jeryu_slug" =~ ^[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._-]*$ ]] ||
    fail "$name has an invalid hosted repository slug"
  [[ -z "${seen_names[$name]:-}" ]] || fail "manifest contains duplicate repository name: $name"
  seen_names["$name"]=1

  if [[ "$name" == "jeryu" && "${JERYU_CLONE_PORTAL:-0}" != "1" ]]; then
    continue
  fi

  target="${dest}/${name}"
  remote="https://git.neverhuman.org/git/${jeryu_slug}.git"
  if [[ "$plan" == "1" ]]; then
    printf '%s|%s|%s\n' "$name" "$remote" "$target"
    continue
  fi

  [[ ! -L "$target" ]] || fail "refusing symlink repository path for $name"
  if [[ -d "${target}/.git" && ! -L "${target}/.git" ]]; then
    target_real="$(cd "$target" && pwd -P)"
    top="$(git -C "$target" rev-parse --show-toplevel 2>/dev/null || true)"
    [[ -n "$top" && "$(realpath -e -- "$top")" == "$target_real" ]] ||
      fail "$name is not a standalone repository at its expected path"
    origin="$(git -C "$target" config --get remote.origin.url 2>/dev/null || true)"
    [[ "$origin" == "$remote" ]] || fail "$name origin does not match hosted authority"
    [[ -z "$(git -C "$target" status --porcelain=v1)" ]] ||
      fail "$name checkout is dirty; refusing to fetch or merge"
    branch="$(git -C "$target" symbolic-ref --quiet --short HEAD 2>/dev/null || true)"
    [[ -n "$branch" ]] || fail "$name checkout is detached; refusing to update"
    git check-ref-format --branch "$branch" >/dev/null || fail "$name checkout branch is invalid"

    printf 'updating %s from hosted authority\n' "$target"
    git -C "$target" fetch --prune origin
    git -C "$target" merge --ff-only --no-edit "refs/remotes/origin/${branch}"
  elif [[ -e "$target" ]]; then
    fail "refusing to overwrite non-repository path for $name"
  else
    printf 'cloning %s -> %s\n' "$remote" "$target"
    git clone --origin origin -- "$remote" "$target"
  fi
done
