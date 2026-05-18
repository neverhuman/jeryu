#!/usr/bin/env bash
# ops/ci/lib.sh — shared helpers and tool-version pins sourced by all ops/ci lane scripts
set -euo pipefail

# ── Tool version pins (keep in sync with jankurai.yml) ────────────────────
JANKURAI_MIN_VERSION="1.3.0"
ACTIONLINT_VERSION="v1.7.8"

# ── Log helpers ────────────────────────────────────────────────────────────
log() { printf '\033[1;34m[ci]\033[0m %s\n' "$*" >&2; }
ok()  { printf '\033[0;32m[ok]\033[0m %s\n' "$*" >&2; }
die() { printf '\033[0;31m[err]\033[0m %s\n' "$*" >&2; exit 1; }

require_tool() {
  command -v "$1" &>/dev/null \
    || die "Required tool not found: $1 — run scripts/ci-doctor.sh to diagnose"
}

ensure_dirs() {
  mkdir -p \
    target/jankurai/security \
    target/jankurai/proofbind \
    target/jankurai/proofmark \
    target/jankurai/rust \
    target/jankurai/sbom
}
