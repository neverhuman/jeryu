#!/usr/bin/env bash
# ops/ci/deploy-local-lane.sh — atomically install the CI-built release binary
# on the local host and write a deploy receipt.
#
# Usage (called by the deploy-local CI job):
#   bash ops/ci/deploy-local-lane.sh [path/to/jeryu]
#
# Expected environment variables (set by GitLab CI):
#   CI_PIPELINE_ID       — GitLab pipeline ID
#   CI_COMMIT_SHA        — full commit SHA
#   CI_COMMIT_SHORT_SHA  — 8-char short SHA
#   CI_COMMIT_REF_NAME   — branch name ("main")
#   CI_COMMIT_TAG        — tag name if triggered by a tag push
#
# Binary install paths (both updated atomically via temp-file + rename):
#   ~/.jeryu/bin/jeryu   — primary (must be on PATH)
#   ~/.cargo/bin/jeryu   — secondary (updated if writable)
#
# Rollback:
#   mv ~/.jeryu/bin/jeryu.prev ~/.jeryu/bin/jeryu
set -euo pipefail

BINARY="${1:-target/release/jeryu}"
CHECKSUM="${BINARY}.sha256"
INSTALL_DIR="$HOME/.jeryu/bin"
CARGO_BIN="$HOME/.cargo/bin"
RECEIPT="$HOME/.jeryu/last-deploy.json"

# ── Sanity guards ────────────────────────────────────────────────────────────

# Fail clearly if we're running inside a Docker container — this job requires
# a shell executor runner so it can write to the host filesystem.
if [ -f /.dockerenv ]; then
  echo "╔══════════════════════════════════════════════════════════════════════╗"
  echo "║ ERROR: deploy-local ran inside a Docker container.                  ║"
  echo "║                                                                      ║"
  echo "║ This job requires a shell executor GitLab runner tagged              ║"
  echo "║ 'jeryu-default' so it can write to ~/.jeryu/bin/ on the host.       ║"
  echo "║                                                                      ║"
  echo "║ Fix option A — add a shell runner:                                   ║"
  echo "║   sudo gitlab-runner register \\                                     ║"
  echo "║     --executor shell --tag-list jeryu-default                        ║"
  echo "║                                                                      ║"
  echo "║ Fix option B — add a volume mount to the Docker runner:              ║"
  echo "║   In /etc/gitlab-runner/config.toml under [[runners]]:               ║"
  echo "║   [runners.docker]                                                   ║"
  echo "║     volumes = [\"/home/ubuntu/.jeryu/bin:/home/ubuntu/.jeryu/bin\"]  ║"
  echo "╚══════════════════════════════════════════════════════════════════════╝"
  exit 1
fi

# Binary must exist
if [ ! -f "$BINARY" ]; then
  echo "ERROR: release binary not found at $BINARY"
  echo "Make sure the build-release job artifact was downloaded."
  exit 1
fi

# Checksum file must exist
if [ ! -f "$CHECKSUM" ]; then
  echo "ERROR: checksum file not found at $CHECKSUM"
  exit 1
fi

# ── Verify integrity ─────────────────────────────────────────────────────────
echo "→ Verifying SHA-256 checksum..."
sha256sum -c "$CHECKSUM"
echo "  ✓ Checksum OK"

# ── Atomic install to ~/.jeryu/bin/ ─────────────────────────────────────────
mkdir -p "$INSTALL_DIR"

# Back up current binary for rollback
if [ -f "$INSTALL_DIR/jeryu" ]; then
  cp "$INSTALL_DIR/jeryu" "$INSTALL_DIR/jeryu.prev"
  echo "→ Backed up previous binary to $INSTALL_DIR/jeryu.prev"
fi

echo "→ Installing to $INSTALL_DIR/jeryu ..."
cp "$BINARY" "$INSTALL_DIR/jeryu.new"
chmod 755 "$INSTALL_DIR/jeryu.new"
mv "$INSTALL_DIR/jeryu.new" "$INSTALL_DIR/jeryu"   # atomic on same filesystem
echo "  ✓ Installed"

# ── Mirror to ~/.cargo/bin/ if writable ─────────────────────────────────────
if [ -d "$CARGO_BIN" ] && [ -w "$CARGO_BIN" ]; then
  echo "→ Mirroring to $CARGO_BIN/jeryu ..."
  cp "$INSTALL_DIR/jeryu" "$CARGO_BIN/jeryu.new"
  mv "$CARGO_BIN/jeryu.new" "$CARGO_BIN/jeryu"
  echo "  ✓ Mirrored"
fi

# ── Confirm installed version ────────────────────────────────────────────────
INSTALLED_VERSION=$("$INSTALL_DIR/jeryu" --version 2>&1)
echo ""
echo "╔══════════════════════════════════════════════════════════════════════╗"
printf "║  %-68s║\n" "Deployed: $INSTALLED_VERSION"
printf "║  %-68s║\n" "Pipeline: ${CI_PIPELINE_ID:-n/a}  Commit: ${CI_COMMIT_SHORT_SHA:-n/a}"
echo "╚══════════════════════════════════════════════════════════════════════╝"
echo ""

# ── Write deploy receipt ─────────────────────────────────────────────────────
mkdir -p "$(dirname "$RECEIPT")"
cat > "$RECEIPT" <<EOF
{
  "version": "$INSTALLED_VERSION",
  "pipeline_id": "${CI_PIPELINE_ID:-unknown}",
  "commit_sha": "${CI_COMMIT_SHA:-unknown}",
  "commit_short_sha": "${CI_COMMIT_SHORT_SHA:-unknown}",
  "ref": "${CI_COMMIT_REF_NAME:-unknown}",
  "tag": "${CI_COMMIT_TAG:-}",
  "deployed_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "binary_path": "$INSTALL_DIR/jeryu",
  "previous_backup": "$INSTALL_DIR/jeryu.prev"
}
EOF

echo "→ Receipt written to $RECEIPT"
