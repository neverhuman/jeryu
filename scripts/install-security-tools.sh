#!/usr/bin/env bash
# Install the native security toolchain required by Jeryu's security lane.
# The workflow jobs and local parity checks call this so the security proof
# does not depend on Docker fallback containers.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
. "$REPO_ROOT/ops/ci/lib.sh"

if [ -z "${SECURITY_TOOLS_PREFIX:-}" ] && [ "$(id -u 2>/dev/null || echo 1)" = "0" ]; then
  PREFIX="/usr/local"
else
  PREFIX="${SECURITY_TOOLS_PREFIX:-$HOME/.local}"
fi
BIN_DIR="$PREFIX/bin"
ACTIONLINT_VERSION_NUM="${ACTIONLINT_VERSION#v}"
TMP_ROOT="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "install-security-tools: missing required tool: $1" >&2
    exit 1
  }
}

ensure_bin_dir_on_path() {
  case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
      export PATH="$BIN_DIR:$PATH"
      if [ -n "${GITHUB_PATH:-}" ]; then
        printf '%s\n' "$BIN_DIR" >> "$GITHUB_PATH"
      fi
      ;;
  esac
}

verify_version() {
  local label="$1"
  local expected="$2"
  shift 2

  local output
  if ! output="$("$@" 2>&1)"; then
    echo "install-security-tools: $label version check failed" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi

  case "$output" in
    *"$expected"*) ;;
    *)
      echo "install-security-tools: expected $label $expected, got: $output" >&2
      exit 1
      ;;
  esac

  printf 'install-security-tools: verified %s (%s)\n' "$label" "$output"
}

install_gitleaks() {
  need curl
  need tar
  mkdir -p "$BIN_DIR"
  local archive="$TMP_ROOT/gitleaks.tar.gz"
  local extract_dir="$TMP_ROOT/gitleaks"
  mkdir -p "$extract_dir"
  curl -fsSL \
    -o "$archive" \
    "https://github.com/gitleaks/gitleaks/releases/download/v${GITLEAKS_VERSION}/gitleaks_${GITLEAKS_VERSION}_linux_x64.tar.gz"
  tar -xzf "$archive" -C "$extract_dir" gitleaks
  install -m 0755 "$extract_dir/gitleaks" "$BIN_DIR/gitleaks"
  verify_version "gitleaks" "$GITLEAKS_VERSION" gitleaks version
}

install_cargo_deny() {
  need cargo
  mkdir -p "$BIN_DIR"
  cargo install --force --locked --root "$PREFIX" \
    cargo-deny --version "$CARGO_DENY_VERSION" >/dev/null
  verify_version "cargo-deny" "$CARGO_DENY_VERSION" cargo-deny --version
}

install_actionlint() {
  need curl
  need tar
  mkdir -p "$BIN_DIR"
  local archive="$TMP_ROOT/actionlint.tar.gz"
  local extract_dir="$TMP_ROOT/actionlint"
  mkdir -p "$extract_dir"
  curl -fsSL \
    -o "$archive" \
    "https://github.com/rhysd/actionlint/releases/download/v${ACTIONLINT_VERSION_NUM}/actionlint_${ACTIONLINT_VERSION_NUM}_linux_amd64.tar.gz"
  tar -xzf "$archive" -C "$extract_dir" actionlint
  install -m 0755 "$extract_dir/actionlint" "$BIN_DIR/actionlint"
  verify_version "actionlint" "$ACTIONLINT_VERSION_NUM" actionlint --version
}

install_syft() {
  need curl
  need tar
  mkdir -p "$BIN_DIR"
  local archive="$TMP_ROOT/syft.tar.gz"
  local extract_dir="$TMP_ROOT/syft"
  mkdir -p "$extract_dir"
  curl -fsSL \
    -o "$archive" \
    "https://github.com/anchore/syft/releases/download/v${SYFT_VERSION}/syft_${SYFT_VERSION}_linux_amd64.tar.gz"
  tar -xzf "$archive" -C "$extract_dir" syft
  install -m 0755 "$extract_dir/syft" "$BIN_DIR/syft"
  verify_version "syft" "$SYFT_VERSION" syft --version
}

ensure_bin_dir_on_path
install_cargo_deny
install_actionlint
install_gitleaks
install_syft

printf 'install-security-tools: installed native security toolchain in %s\n' "$BIN_DIR"
