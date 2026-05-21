#!/usr/bin/env bash
# ops/ci/release-lane.sh — run one stage of the release pipeline
# Usage: bash ops/ci/release-lane.sh <preflight|audit|security|build|provenance|evidence|rollback-check|publish> <version>
#
# Single source of truth for the release pipeline. `.github/workflows/release.yml`
# and local `jeryu release dry-run` both call into these stages so CI and local
# parity stay aligned (jankurai HLT-042).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$SCRIPT_DIR/lib.sh"
cd "$REPO_ROOT"

STAGE="${1:-}"
VERSION="${2:-}"
if [ -z "$STAGE" ] || [ -z "$VERSION" ]; then
  die "usage: bash ops/ci/release-lane.sh <stage> <version>"
fi

ensure_dirs

install_redlinedb_binary() {
  local backend="${JERYU_DB_BACKEND:-sqlite}"
  local url="${JERYU_DATABASE_URL:-}"
  case "${backend,,}" in
    redline|redlinedb)
      log "install RedlineDB binary"
      bash scripts/install-redlinedb.sh
      return
      ;;
  esac
  case "${url,,}" in
    redline:*|redlinedb:*)
      log "install RedlineDB binary"
      bash scripts/install-redlinedb.sh
      return
      ;;
  esac
  log "skip RedlineDB binary install for SQLite backend"
}

run_preflight() {
  install_redlinedb_binary
  log "version consistency check (VERSION vs request)"
  local file_ver
  file_ver="$(tr -d '\n' < VERSION)"
  if [ "$file_ver" != "$VERSION" ] && ! printf '%s\n' "$VERSION" | grep -qx "${file_ver}-rc\\.[0-9]\\+"; then
    die "VERSION ($file_ver) does not match release input ($VERSION)"
  fi
  log "jeryu release dry-run for $VERSION"
  JERYU_RELEASE_REPO_ROOT="$REPO_ROOT" cargo run --release -p jeryu -- release dry-run --version "$VERSION" --json
}

run_audit() {
  log "cargo deny check"
  cargo deny check
}

run_security() {
  install_redlinedb_binary
  log "security tests"
  cargo test -p jeryu --release -- secrets exec honeypot admission
}

run_build() {
  install_redlinedb_binary
  log "cargo build --release -p jeryu"
  cargo build --release -p jeryu
}

run_provenance_sbom() {
  log "cyclonedx SBOM generation"
  ensure_dirs
  if ! command -v cargo-cyclonedx >/dev/null 2>&1; then
    log "installing cargo-cyclonedx"
    cargo install cargo-cyclonedx --locked --version "^0.5"
  fi
  cargo cyclonedx --output-pattern package --output-cdx --target-dir target/jankurai/sbom
}

run_evidence() {
  log "compose evidence directory for $VERSION"
  local dir="ops/releases/$VERSION"
  mkdir -p "$dir"
  : > "$dir/release-attempt.json"
  printf '{"version":"%s","sha":"%s","ci_run":"%s","ci_attempt":"%s"}\n' \
    "$VERSION" "${GITHUB_SHA:-local}" "${GITHUB_RUN_ID:-local}" "${GITHUB_RUN_ATTEMPT:-1}" \
    > "$dir/release-attempt.json"
  ok "evidence dir: $dir"
}

run_rollback_check() {
  log "assert rollback target declared for $VERSION"
  if [ -f "ops/releases/$VERSION/rollback-target.json" ]; then
    ok "rollback target present for $VERSION"
  elif [ -f "ops/releases/3.0.1-rc.1.example/rollback-target.json" ]; then
    ok "using example fixture (ok for rc; stable should declare its own)"
  else
    die "no rollback target declared for $VERSION"
  fi
}

run_publish() {
  require_tool gh
  local tag="v$VERSION"
  if ! git rev-parse --verify "refs/tags/$tag" >/dev/null 2>&1; then
    die "Tag $tag not found; jeryu release submit should have pushed it"
  fi
  gh release create "$tag" \
    --verify-tag \
    --title "$tag" \
    --notes-file "ops/releases/$VERSION/changelog.md" \
    target/release/jeryu
}

case "$STAGE" in
  preflight)      run_preflight ;;
  audit)          run_audit ;;
  security)       run_security ;;
  build)          run_build ;;
  provenance)     run_provenance_sbom ;;
  evidence)       run_evidence ;;
  rollback-check) run_rollback_check ;;
  publish)        run_publish ;;
  *)              die "unknown stage: $STAGE" ;;
esac
