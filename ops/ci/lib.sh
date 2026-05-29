#!/usr/bin/env bash
# ops/ci/lib.sh — shared helpers and tool-version pins sourced by all ops/ci lane scripts
set -euo pipefail

# ── Tool version pins (keep in sync with scripts/jankurai-manifest.json) ──
JANKURAI_REQUIRED_VERSION="1.5.1"
ACTIONLINT_VERSION="v1.7.8"

# ── Log helpers ────────────────────────────────────────────────────────────
log() { printf '\033[1;34m[ci]\033[0m %s\n' "$*" >&2; }
ok()  { printf '\033[0;32m[ok]\033[0m %s\n' "$*" >&2; }
die() { printf '\033[0;31m[err]\033[0m %s\n' "$*" >&2; exit 1; }

require_tool() {
  command -v "$1" &>/dev/null \
    || die "Required tool not found: $1 — run scripts/ci-doctor.sh to diagnose"
}

install_ci_packages() {
  if [ -z "${CI:-}" ] && [ -z "${GITLAB_CI:-}" ]; then
    return 0
  fi

  if [ "$(id -u)" != "0" ]; then
    die "CI package bootstrap requires root; run the lane inside the GitLab CI container"
  fi

  local packages=("$@")
  if [ "${#packages[@]}" -eq 0 ]; then
    return 0
  fi

  log "installing CI packages: ${packages[*]}"
  export DEBIAN_FRONTEND=noninteractive
  local apt_cache_root="${APT_CACHE_DIR:-/cache/apt-archives}"
  local apt_cache_dir="$apt_cache_root/${CI_JOB_ID:-$$}"
  mkdir -p "$apt_cache_dir/partial"
  apt-get update -qq
  apt-get -o Dir::Cache::archives="$apt_cache_dir" \
    install -y -qq --no-install-recommends "${packages[@]}"
  rm -rf "$apt_cache_dir"
}

require_jankurai() {
  require_tool jankurai
  local version_output
  version_output="$(jankurai --version 2>/dev/null)" \
    || die "jankurai --version failed — run bash scripts/install-jankurai.sh"
  case "$version_output" in
    "jankurai $JANKURAI_REQUIRED_VERSION") ;;
    *)
      die "Required jankurai $JANKURAI_REQUIRED_VERSION, got: $version_output — run bash scripts/install-jankurai.sh"
      ;;
  esac
}

ensure_dirs() {
  mkdir -p \
    target/jankurai/security \
    target/jankurai/proofbind \
    target/jankurai/proofmark \
    target/jankurai/rust \
    target/jankurai/sbom
}
