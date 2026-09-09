#!/usr/bin/env bash
# Shared source identity for build and installation. Paths are NUL-delimited.
set -euo pipefail

source_digest() {
  (
    cd "$1"
    git ls-files -z --cached --others --exclude-standard |
      LC_ALL=C sort -zu |
      while IFS= read -r -d '' path; do
        if [[ -L "$path" ]]; then
          printf 'source input must not be a symlink: %s\n' "$path" >&2
          exit 1
        fi
        if [[ -f "$path" ]]; then sha256sum --zero -- "$path"; fi
      done |
      sha256sum | cut -d ' ' -f 1
  )
}
