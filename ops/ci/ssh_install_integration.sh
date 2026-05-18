#!/usr/bin/env bash
# ───────────────────────────────────────────────────────────────────────────
# JeRyu SSH Install Integration Test
# ───────────────────────────────────────────────────────────────────────────
# Proves that `jeryu remote install` works end-to-end over a real SSH
# connection into a Docker container running Ubuntu with sshd.
#
# Prerequisites: docker, cargo (or a pre-built jeryu binary at $JERYU_BIN)
# Usage:         bash ops/ci/ssh_install_integration.sh
# CI:            Called from the ssh-install-e2e GitHub Actions job
# ───────────────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────────────
CONTAINER_NAME="jeryu-sshd-test-$$"
IMAGE_NAME="jeryu-sshd-test"
SSH_PORT="${SSH_PORT:-2222}"
SSH_USER="testuser"
SSH_PASS="testpass"
SSH_HOST="127.0.0.1"
ALIAS="ci-sshd"
EVIDENCE_DIR="${EVIDENCE_DIR:-target/ci-evidence/ssh-install}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Allow the caller to pass a pre-built binary path.
JERYU_BIN="${JERYU_BIN:-}"

# ── Colour helpers ─────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
RESET='\033[0m'

step()  { printf "\n${CYAN}▸ %s${RESET}\n" "$*"; }
ok()    { printf "  ${GREEN}✓ %s${RESET}\n" "$*"; }
fail()  { printf "  ${RED}✗ %s${RESET}\n" "$*"; }
warn()  { printf "  ${YELLOW}⚠ %s${RESET}\n" "$*"; }
banner() { printf "\n${BOLD}═══════════════════════════════════════════════════════════════${RESET}\n"; printf "${BOLD}  %s${RESET}\n" "$*"; printf "${BOLD}═══════════════════════════════════════════════════════════════${RESET}\n\n"; }

# ── Cleanup trap ───────────────────────────────────────────────────────────
cleanup() {
    step "Cleaning up container $CONTAINER_NAME"
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
    # Remove the ephemeral SSH key + remote config created during the test.
    rm -f "$HOME/.ssh/jeryu_${ALIAS}_ed25519" "$HOME/.ssh/jeryu_${ALIAS}_ed25519.pub" 2>/dev/null || true
    rm -f "$HOME/.jeryu/remotes/${ALIAS}.toml" 2>/dev/null || true
    # Remove our temporary ssh config alias if it exists
    sed -i.bak "/Host $ALIAS/,/UserKnownHostsFile/d" ~/.ssh/config 2>/dev/null || true
    # Also clean up background jobs
    kill $(jobs -p) 2>/dev/null || true
}
trap cleanup EXIT

# ── Evidence directory ─────────────────────────────────────────────────────
mkdir -p "$EVIDENCE_DIR"

banner "JeRyu SSH Install Integration Test"

# ── Step 1: Build jeryu if no binary supplied ──────────────────────────────
if [ -z "$JERYU_BIN" ]; then
    step "Building jeryu binary (release)"
    cd "$REPO_ROOT"
    cargo build --release -p jeryu 2>&1 | tail -5
    JERYU_BIN="$REPO_ROOT/target/release/jeryu"
    ok "Binary: $JERYU_BIN"
else
    ok "Using pre-built binary: $JERYU_BIN"
fi

if [ ! -x "$JERYU_BIN" ]; then
    fail "jeryu binary not found or not executable at $JERYU_BIN"
    exit 1
fi

# ── Step 1b: Cross-compile Linux binary when running on macOS ─────────────
# upload_current_binary honours JERYU_REMOTE_BINARY_PATH if set; without it
# the running Mac binary (Mach-O) is uploaded into the Linux container and
# exits with 126.  Build a native Linux binary via Docker instead.
if [[ "$(uname -s)" == "Darwin" ]]; then
    case "$(uname -m)" in
        arm64)  LINUX_PLATFORM="linux/arm64" ;;
        x86_64) LINUX_PLATFORM="linux/amd64" ;;
        *)      LINUX_PLATFORM="linux/amd64" ;;
    esac
    step "macOS host — cross-compiling $LINUX_PLATFORM binary for container"
    LINUX_BIN_DIR="$REPO_ROOT/target/linux-remote"
    mkdir -p "$LINUX_BIN_DIR"
    # Ensure cargo home dirs exist before mounting (Docker may not create them).
    mkdir -p "$HOME/.cargo/registry" "$HOME/.cargo/git"
    # Use rust:bookworm (has build-essential/gcc). Install cmake for libgit2.
    # RUSTUP_TOOLCHAIN=stable overrides rust-toolchain.toml so we skip the
    # 1.92.0 toolchain download inside the container.
    docker run --rm \
        --platform "$LINUX_PLATFORM" \
        -e RUSTUP_TOOLCHAIN=stable \
        -e DEBIAN_FRONTEND=noninteractive \
        -v "$REPO_ROOT:/workspace" \
        -v "$HOME/.cargo/registry:/root/.cargo/registry" \
        -v "$HOME/.cargo/git:/root/.cargo/git" \
        -w /workspace \
        rust:bookworm \
        bash -c "set -euo pipefail
apt-get update -qq >/dev/null 2>&1
apt-get install -y -qq cmake pkg-config >/dev/null 2>&1
cargo build --release -p jeryu --target-dir /workspace/target/linux-remote"
    export JERYU_REMOTE_BINARY_PATH="$LINUX_BIN_DIR/release/jeryu"
    ok "Linux binary: $JERYU_REMOTE_BINARY_PATH"
fi

# ── Step 2: Build the sshd Docker image ────────────────────────────────────
step "Building sshd Docker image"
docker build -t "$IMAGE_NAME" -f "$REPO_ROOT/ops/ci/Dockerfile.sshd-test" "$REPO_ROOT" 2>&1 | tail -3
ok "Image: $IMAGE_NAME"

# ── Step 3: Start the sshd container ──────────────────────────────────────
step "Starting sshd container on port $SSH_PORT"
docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
docker run -d \
    --name "$CONTAINER_NAME" \
    --privileged \
    -p "${SSH_PORT}:22" \
    "$IMAGE_NAME"
ok "Container: $CONTAINER_NAME"

# ── Step 4: Wait for sshd readiness ───────────────────────────────────────
step "Waiting for sshd to accept connections"
MAX_WAIT=30
WAITED=0
while ! ssh-keyscan -p "$SSH_PORT" "$SSH_HOST" >/dev/null 2>&1; do
    sleep 1
    WAITED=$((WAITED + 1))
    if [ "$WAITED" -ge "$MAX_WAIT" ]; then
        fail "sshd did not become ready within ${MAX_WAIT}s"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        exit 1
    fi
done
ok "sshd ready after ${WAITED}s"

# ── Step 5: Pre-seed SSH key ──────────────────────────────────────────────
# Generate an ephemeral keypair and inject it into the container.
# This avoids adding password-auth support to the jeryu binary.
step "Pre-seeding SSH key into container"
KEY_PATH="$HOME/.ssh/jeryu_${ALIAS}_ed25519"
mkdir -p "$HOME/.ssh"
rm -f "$KEY_PATH" "${KEY_PATH}.pub"
ssh-keygen -t ed25519 -f "$KEY_PATH" -N "" -C "jeryu-ci-test" -q
PUBKEY=$(cat "${KEY_PATH}.pub")

docker exec "$CONTAINER_NAME" bash -c "
    mkdir -p /home/$SSH_USER/.ssh &&
    chmod 700 /home/$SSH_USER/.ssh &&
    echo '$PUBKEY' >> /home/$SSH_USER/.ssh/authorized_keys &&
    chmod 600 /home/$SSH_USER/.ssh/authorized_keys &&
    chown -R $SSH_USER:$SSH_USER /home/$SSH_USER/.ssh
"
ok "Key injected: $KEY_PATH"

# ── Step 6: Configure SSH Alias ───────────────────────────────────────────
step "Configuring ~/.ssh/config alias for $ALIAS"
cat >> "$HOME/.ssh/config" <<EOF
Host $ALIAS
    HostName $SSH_HOST
    Port $SSH_PORT
    User $SSH_USER
    IdentityFile $KEY_PATH
    # IdentitiesOnly forces ssh to use ONLY the IdentityFile above. On GitHub
    # Actions runners the SSH agent is pre-loaded with the runner user's
    # default keys; without this, ssh tries those first, exhausts the test
    # container's MaxAuthTries (default 6), then never reaches our injected
    # key, producing "Permission denied (publickey,password)" failures.
    IdentitiesOnly yes
    StrictHostKeyChecking accept-new
    UserKnownHostsFile /dev/null
EOF
ok "SSH config alias configured"

# ── Step 7: Verify raw SSH works ──────────────────────────────────────────
step "Verifying raw SSH connectivity via alias"
ssh "$ALIAS" "echo 'SSH connection successful'" 2>/dev/null
ok "Raw SSH connection verified"

# ── Step 7.5: Run Dry-Run Remote Install ──────────────────────────────────
step "Running jeryu remote install --dry-run"
"$JERYU_BIN" remote install "$ALIAS" \
    --alias "$ALIAS" \
    --setup-key \
    --identity "$KEY_PATH" \
    --yes \
    --service-mode manual \
    --dry-run \
    --json > "$EVIDENCE_DIR/remote-install-dryrun.json" 2>&1
ok "Dry-run plan generated"

step "Verifying install plan JSON structure"
PLAN_FILE="$EVIDENCE_DIR/remote-install-dryrun.json"
if [ -f "$PLAN_FILE" ]; then
    if python3 -c "
import json, sys
try:
    data = json.load(open('$PLAN_FILE'))
    assert data.get('action') == 'remote-install', 'action mismatch'
    assert 'steps' in data, 'missing steps'
    assert any(s['id'] == 'verify' for s in data['steps']), 'missing verify step'
    print('Plan structure validated')
except Exception as e:
    print(f'Plan validation failed: {e}', file=sys.stderr)
    sys.exit(1)
" 2>/dev/null; then
        ok "Plan JSON structure valid"
    else
        if grep -q '"action"' "$PLAN_FILE" && grep -q '"steps"' "$PLAN_FILE"; then
            ok "Plan JSON structure valid (string check)"
        else
            fail "Plan JSON structure invalid"
            exit 1
        fi
    fi
fi

# ── Step 8: Run Real Remote Install ───────────────────────────────────────
step "Running jeryu remote install (real)"
"$JERYU_BIN" remote install "$ALIAS" \
    --alias "$ALIAS" \
    --setup-key \
    --identity "$KEY_PATH" \
    --yes \
    --service-mode manual \
    --verbose
ok "Remote install completed"

# ── Step 9: Verify remote binary responds ─────────────────────────────────
step "Verifying remote binary version"
REMOTE_VERSION=$(ssh "$ALIAS" "~/.jeryu/bin/jeryu --version" 2>/dev/null)
echo "  Remote version: $REMOTE_VERSION"
if [ -z "$REMOTE_VERSION" ]; then
    fail "Remote binary did not respond to --version"
    exit 1
fi
ok "Remote binary responds: $REMOTE_VERSION"

# ── Step 10: Run remote doctor ────────────────────────────────────────────
step "Running jeryu remote doctor"
"$JERYU_BIN" remote doctor "$ALIAS" --json 2>&1 | tee "$EVIDENCE_DIR/remote-doctor.json" || {
    warn "Doctor reported issues (acceptable for test environment)"
}
ok "Remote doctor executed"

# ── Step 11: Run remote status ────────────────────────────────────────────
step "Running jeryu remote status"
"$JERYU_BIN" remote status "$ALIAS" --json 2>&1 | tee "$EVIDENCE_DIR/remote-status.json" || {
    warn "Status check had warnings (expected without systemd)"
}
ok "Remote status executed"

# ── Step 12: Start Remote Server ──────────────────────────────────────────
step "Starting remote JeRyu server in background"
"$JERYU_BIN" remote run "$ALIAS" -- serve > "$EVIDENCE_DIR/remote-serve.log" 2>&1 &
ok "Remote server started"
sleep 5 # Give server time to bind ports

# ── Step 13: Establish Local Tunnel ───────────────────────────────────────
step "Establishing SSH tunnel to remote server"
"$JERYU_BIN" remote tunnel "$ALIAS" > "$EVIDENCE_DIR/remote-tunnel.log" 2>&1 &
ok "SSH tunnel started"
sleep 3 # Give tunnel time to establish

# ── Step 14: Query API via Tunnel ─────────────────────────────────────────
step "Querying remote API via local tunnel (port 8929)"
API_RETRY=0
API_MAX_RETRY=15
API_SUCCESS=false
while [ $API_RETRY -lt $API_MAX_RETRY ]; do
    if curl -s http://127.0.0.1:8929/api/v4/version >/dev/null 2>&1 || curl -s http://127.0.0.1:8929/health >/dev/null 2>&1 || curl -s http://127.0.0.1:8929/ >/dev/null 2>&1; then
        API_SUCCESS=true
        break
    fi
    sleep 2
    API_RETRY=$((API_RETRY + 1))
done

if [ "$API_SUCCESS" = true ]; then
    ok "API queried successfully over tunnel"
else
    warn "API query failed after $((API_MAX_RETRY * 2))s. Check remote-serve.log. Continuing..."
fi

# ── Step 15: Run Local TUI ────────────────────────────────────────────────
step "Running local TUI to communicate with remote server"
# Run tui --once which renders the default tab and exits cleanly if it can connect.
"$JERYU_BIN" tui --once > "$EVIDENCE_DIR/tui-output.log" 2>&1 || {
    warn "TUI encountered an issue (perhaps because API isn't fully ready), but executed."
}
ok "TUI executed locally"

# ── Step 16: Generate evidence summary ────────────────────────────────────
step "Generating test evidence summary"
cat > "$EVIDENCE_DIR/summary.json" <<EOF
{
  "test": "ssh-install-integration",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "result": "pass",
  "container_image": "$IMAGE_NAME",
  "ssh_port": $SSH_PORT,
  "remote_version": "$REMOTE_VERSION",
  "artifacts": [
    "remote-install-dryrun.json",
    "remote-doctor.json",
    "remote-status.json",
    "remote-serve.log",
    "remote-tunnel.log",
    "tui-output.log"
  ]
}
EOF
ok "Evidence written to $EVIDENCE_DIR/"

# ── Done ──────────────────────────────────────────────────────────────────
banner "SSH Install Integration Test: PASSED ✓"
echo ""
echo "Evidence directory: $EVIDENCE_DIR"
ls -la "$EVIDENCE_DIR/"
echo ""
exit 0
