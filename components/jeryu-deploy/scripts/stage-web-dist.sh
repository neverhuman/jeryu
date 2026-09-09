#!/usr/bin/env bash
# Stage the built SPA from the sibling jeryu-web checkout into apps/web/dist so
# the fused binary's relative --spa-dir default works and release tags are
# self-contained. When the sibling is absent (release-from-tag), the vendored
# copy already in this repo is used as-is.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$repo_root/../jeryu-web/apps/web/dist"
dst="$repo_root/apps/web/dist"
if [ -d "$src/assets" ]; then
  mkdir -p "$dst"
  rsync -a --delete "$src/" "$dst/"
  echo "staged SPA dist: $src -> $dst"
else
  [ -d "$dst/assets" ] && echo "sibling dist absent; using vendored copy at $dst" || {
    echo "no SPA dist available (build jeryu-web first)" >&2
    exit 1
  }
fi
