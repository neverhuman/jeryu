#!/usr/bin/env bash
set -euo pipefail
find . \
  -path './target' -prune -o \
  -path './.git' -prune -o \
  -name '._*' -prune -o \
  -type f -print \
  | sed 's#^./##' \
  | sort > FILE_TREE.txt
