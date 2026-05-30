#!/usr/bin/env bash
# ops/ci/install-mold.sh — best-effort mold bootstrap for CI workflows
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
. "$script_dir/lib.sh"

if command -v mold >/dev/null 2>&1; then
  log "mold already installed"
elif sudo -n true 2>/dev/null; then
  sudo apt-get install -y mold
else
  log "no passwordless sudo; skipping mold"
fi
