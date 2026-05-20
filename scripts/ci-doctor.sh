#!/usr/bin/env bash
# scripts/ci-doctor.sh — verify local environment matches CI tool requirements
# Usage: bash scripts/ci-doctor.sh
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$SCRIPT_DIR/../ops/ci/lib.sh"

PASS=0
FAIL=0

check() {
  local name="$1" cmd="$2"
  if eval "$cmd" &>/dev/null 2>&1; then
    ok "$name"
    PASS=$((PASS + 1))
  else
    printf '\033[0;31m[fail]\033[0m %s — not found or wrong version\n' "$name" >&2
    FAIL=$((FAIL + 1))
  fi
}

log "checking tools required by ops/ci lane scripts"
check "cargo"        "cargo --version"
check "rustup"       "rustup --version"
check "jankurai"     "jankurai --version"
check "cargo-nextest" "cargo nextest --version"
check "git"          "git --version"
check "docker"       "docker --version"
check "redlinedb"    "bash scripts/install-redlinedb.sh"
check "actionlint"   "actionlint --version"

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
