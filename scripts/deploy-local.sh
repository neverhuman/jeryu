#!/usr/bin/env bash
# scripts/deploy-local.sh — build the release binary, install globally on this
# host, then verify the install with a post-install smoke + version check.
#
# Usage:
#   bash scripts/deploy-local.sh           # build + install + verify
#   bash scripts/deploy-local.sh --check   # build + verify artifact; skip install step
#   bash scripts/deploy-local.sh --verify  # verify only (binary already installed)
#
# Exit codes:
#   0 = success
#   1 = build failure
#   2 = install failure
#   3 = verification failure
#
# The install destination is determined by:
#   1. $JERYU_INSTALL_PREFIX, if set (default destination = $PREFIX/bin/jeryu)
#   2. $HOME/.local/bin if writable
#   3. /usr/local/bin (requires sudo if not writable)

set -euo pipefail

MODE="${1:-full}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ── colors (only if attached to a tty) ─────────────────────────────────────
if [ -t 1 ]; then
    GREEN=$'\033[32m'; RED=$'\033[31m'; YELLOW=$'\033[33m'; BLUE=$'\033[34m'; RESET=$'\033[0m'
else
    GREEN=""; RED=""; YELLOW=""; BLUE=""; RESET=""
fi

step() { printf '%s━━ %s%s\n' "$BLUE" "$1" "$RESET"; }
ok()   { printf '%s✓%s %s\n' "$GREEN" "$RESET" "$1"; }
fail() { printf '%s✗%s %s\n' "$RED" "$RESET" "$1" >&2; }
warn() { printf '%s!%s %s\n' "$YELLOW" "$RESET" "$1" >&2; }

# ── resolve install destination ────────────────────────────────────────────
# Priority:
#   1. $JERYU_INSTALL_PREFIX/bin (explicit override)
#   2. The directory containing whatever `which jeryu` resolves to (so a
#      pre-existing system install at e.g. ~/.jeryu/bin or /usr/local/bin
#      gets replaced in place — that's the path the user's PATH already
#      points at)
#   3. ~/.jeryu/bin if it exists (the project's conventional install root)
#   4. ~/.local/bin (XDG-friendly user-local default)
#   5. /usr/local/bin (system-wide; will need sudo)
resolve_install_dir() {
    if [ -n "${JERYU_INSTALL_PREFIX:-}" ]; then
        echo "${JERYU_INSTALL_PREFIX}/bin"; return
    fi
    # Prefer the directory the existing system 'jeryu' resolves to, so the
    # deploy replaces what's on PATH today rather than shadowing it.
    if command -v jeryu >/dev/null 2>&1; then
        local existing
        existing="$(command -v jeryu)"
        # Don't accidentally install over the repo's own target/release/jeryu
        case "$existing" in
            "$REPO_ROOT"/target/*) ;;
            *)
                local existing_dir
                existing_dir="$(dirname "$existing")"
                if [ -w "$existing_dir" ] || command -v sudo >/dev/null 2>&1; then
                    echo "$existing_dir"; return
                fi
                ;;
        esac
    fi
    if [ -d "$HOME/.jeryu/bin" ]; then
        echo "$HOME/.jeryu/bin"; return
    fi
    if [ -d "$HOME/.local/bin" ] && [ -w "$HOME/.local/bin" ]; then
        echo "$HOME/.local/bin"; return
    fi
    if [ ! -d "$HOME/.local/bin" ]; then
        mkdir -p "$HOME/.local/bin" 2>/dev/null && {
            echo "$HOME/.local/bin"; return
        }
    fi
    echo "/usr/local/bin"
}

INSTALL_DIR="$(resolve_install_dir)"
INSTALL_PATH="$INSTALL_DIR/jeryu"
READY_PATH="$INSTALL_PATH"

# ── 1. Build the release binary (skip if --verify only) ────────────────────
do_build() {
    step "Build release binary"
    if ! cargo build --release --features web -p jeryu --bin jeryu 2>&1 | tail -3; then
        fail "release build failed"
        exit 1
    fi
    if [ ! -x "$REPO_ROOT/target/release/jeryu" ]; then
        fail "release binary missing at target/release/jeryu after build"
        exit 1
    fi
    local sz
    sz=$(du -h "$REPO_ROOT/target/release/jeryu" | awk '{print $1}')
    ok "release binary built ($sz)"
}

# ── helper: stop/start jeryu-web.service if systemd user session is present ──
_web_service_stop() {
    if systemctl --user is-active --quiet jeryu-web.service 2>/dev/null; then
        step "Stop jeryu-web.service for binary swap"
        systemctl --user stop jeryu-web.service || true
    fi
}

_web_service_start() {
    if systemctl --user is-enabled --quiet jeryu-web.service 2>/dev/null; then
        step "Restart jeryu-web.service with new binary"
        systemctl --user start jeryu-web.service && ok "jeryu-web.service started on :8787" || warn "jeryu-web.service start failed (check: systemctl --user status jeryu-web.service)"
    fi
}

# ── 2. Install to the resolved path ─────────────────────────────────────────
do_install() {
    step "Install to $INSTALL_PATH"

    local sudo_cmd=""
    if [ ! -w "$INSTALL_DIR" ]; then
        if command -v sudo >/dev/null 2>&1; then
            warn "$INSTALL_DIR is not writable; using sudo"
            sudo_cmd="sudo"
        else
            fail "$INSTALL_DIR not writable and sudo unavailable"
            exit 2
        fi
    fi

    if [ -f "$INSTALL_PATH" ]; then
        local old_version
        old_version="$($INSTALL_PATH --version 2>/dev/null | head -1 || echo 'unknown')"
        warn "replacing $old_version"
    fi

    _web_service_stop

    if ! $sudo_cmd install -m 0755 "$REPO_ROOT/target/release/jeryu" "$INSTALL_PATH"; then
        fail "install copy failed"
        exit 2
    fi
    ok "installed to $INSTALL_PATH"

    _web_service_start

    # PATH sanity check
    if ! command -v jeryu >/dev/null 2>&1; then
        warn "'jeryu' not on PATH — $INSTALL_DIR may not be in \$PATH"
        warn "add it with: export PATH=\"$INSTALL_DIR:\$PATH\""
    fi
}

# ── 3. Post-install verification ────────────────────────────────────────────
do_verify() {
    local binary_path="${1:-$INSTALL_PATH}"
    local label="${2:-Post-install verification}"

    step "$label"

    if [ ! -x "$binary_path" ]; then
        fail "$binary_path is missing or not executable"
        exit 3
    fi

    # Version check — must match what cargo built
    local installed_version expected_version
    installed_version="$("$binary_path" --version 2>&1 | head -1 || echo 'failed')"
    expected_version="$(grep -m1 '^version = ' "$REPO_ROOT/Cargo.toml" | sed 's/^version = "\(.*\)"/\1/')"

    if [ -z "$installed_version" ] || ! echo "$installed_version" | grep -q "$expected_version"; then
        fail "version mismatch: installed='$installed_version' expected='$expected_version'"
        exit 3
    fi
    ok "version check: $installed_version (matches Cargo.toml $expected_version)"

    # TUI smoke render — should print 'jeryu TUI smoke render ok'
    local smoke
    if ! smoke="$("$binary_path" tui --once --demo 2>&1)"; then
        fail "TUI smoke command failed (exit non-zero)"
        printf '%s\n' "$smoke" | head -10 >&2
        exit 3
    fi
    if ! echo "$smoke" | grep -q "smoke render ok"; then
        fail "TUI smoke render did not emit 'smoke render ok' marker"
        printf '%s\n' "$smoke" | head -10 >&2
        exit 3
    fi
    ok "tui --once --demo: $smoke"

    # Help works
    if ! "$binary_path" --help >/dev/null 2>&1; then
        fail "--help failed"
        exit 3
    fi
    ok "--help returns 0"

    # Reusable check that build.rs embedded git SHA matches HEAD
    local git_head build_sha
    git_head="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo 'no-git')"
    build_sha="$("$binary_path" --version 2>&1 | grep -oE '[a-f0-9]{7,40}' | head -1 || echo 'no-sha')"
    if [ "$git_head" != "no-git" ] && [ "$build_sha" != "no-sha" ]; then
        if echo "$build_sha" | grep -q "^$git_head"; then
            ok "embedded git SHA matches HEAD ($git_head)"
        else
            warn "embedded SHA '$build_sha' does not match git HEAD '$git_head'"
            warn "(this is expected if you rebuilt locally without committing)"
        fi
    fi
}

# ── main ────────────────────────────────────────────────────────────────────
case "$MODE" in
    --verify|verify)
        do_verify
        ;;
    --check|check)
        do_build
        READY_PATH="$REPO_ROOT/target/release/jeryu"
        do_verify "$READY_PATH" "Build artifact verification"
        ;;
    --build-only|build-only)
        do_build
        ;;
    full|"")
        do_build
        do_install
        do_verify
        ;;
    *)
        fail "unknown mode: $MODE"
        echo "Usage: $0 [--check|--verify|--build-only|full]" >&2
        exit 2
        ;;
esac

step "Deploy complete"
ok "$READY_PATH ready"
