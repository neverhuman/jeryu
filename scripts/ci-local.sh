#!/usr/bin/env bash
set -euo pipefail
case "${1:-fast}" in
  fast) ./ops/ci/fast.sh ;;
  full) ./ops/ci/full.sh ;;
  audit) ./ops/ci/audit.sh ;;
  security) ./ops/ci/security.sh ;;
  release) ./ops/ci/release.sh ;;
  *) echo "usage: $0 [fast|full|audit|security|release]" >&2; exit 2 ;;
esac
