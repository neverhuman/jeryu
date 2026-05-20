#!/usr/bin/env bash
# Install the required RedlineDB CLI/native binary from a pinned upstream release
# by default, with checksum verification before extraction.
#
# Use REDLINEDB_INSTALL_MODE=verify only when intentionally checking an
# already-installed binary without network access.
set -euo pipefail

REDLINEDB_INSTALL_MODE="${REDLINEDB_INSTALL_MODE:-release}"
REDLINEDB_VERSION="${REDLINEDB_VERSION:-v1.0.1}"
REPO="${REDLINEDB_REPO:-neverhuman/RedlineDB}"
PREFIX="${REDLINEDB_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
INSTALL_NAME="${REDLINEDB_INSTALL_NAME:-redlinedb}"
REDLINEDB_BIN="${REDLINEDB_BIN:-$BIN_DIR/$INSTALL_NAME}"
if [ "$REDLINEDB_VERSION" = "latest" ]; then
    API_URL="https://api.github.com/repos/${REPO}/releases/latest"
else
    API_URL="https://api.github.com/repos/${REPO}/releases/tags/${REDLINEDB_VERSION}"
fi

need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "install-redlinedb: missing required tool: $1" >&2
        exit 1
    }
}

verify_installed_binary() {
    if [ ! -e "$REDLINEDB_BIN" ]; then
        echo "install-redlinedb: required binary is missing: $REDLINEDB_BIN" >&2
        echo "install-redlinedb: install or symlink RedlineDB at $REDLINEDB_BIN" >&2
        exit 1
    fi

    if [ ! -x "$REDLINEDB_BIN" ]; then
        echo "install-redlinedb: required binary is not executable: $REDLINEDB_BIN" >&2
        echo "install-redlinedb: install or symlink RedlineDB at $REDLINEDB_BIN" >&2
        exit 1
    fi

    if ! version_output="$("$REDLINEDB_BIN" --version 2>&1)"; then
        echo "install-redlinedb: $REDLINEDB_BIN --version failed" >&2
        printf '%s\n' "$version_output" >&2
        echo "install-redlinedb: install or symlink RedlineDB at $REDLINEDB_BIN" >&2
        exit 1
    fi

    printf 'install-redlinedb: verified %s (%s)\n' "$REDLINEDB_BIN" "$version_output"
}

install_from_release() {
    need curl
    need python3

    case "$(uname -s)" in
        Linux) OS_TOKENS="linux" ;;
        Darwin) OS_TOKENS="darwin macos apple" ;;
        MINGW*|MSYS*|CYGWIN*) OS_TOKENS="windows win32 win64 pc" ;;
        *)
            echo "install-redlinedb: unsupported OS: $(uname -s)" >&2
            exit 1
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64) ARCH_TOKENS="x86_64 x64 amd64" ;;
        aarch64|arm64) ARCH_TOKENS="aarch64 arm64" ;;
        armv7l) ARCH_TOKENS="armv7 arm" ;;
        *)
            echo "install-redlinedb: unsupported architecture: $(uname -m)" >&2
            exit 1
            ;;
    esac

    local release_json selection status tag asset_name asset_url checksum_name checksum_url
    local asset_path checksum_path extract_dir binary
    tmp="$(mktemp -d)"
    cleanup() {
        if [ -n "${tmp:-}" ]; then
            rm -rf "$tmp"
        fi
    }
    trap cleanup EXIT

    local auth_args=()
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        auth_args=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
    fi

    release_json="$tmp/release.json"
    curl -fsSL "${auth_args[@]}" \
        -H "Accept: application/vnd.github+json" \
        -H "X-GitHub-Api-Version: 2022-11-28" \
        "$API_URL" > "$release_json"

    selection="$(
        REDLINEDB_OS_TOKENS="$OS_TOKENS" REDLINEDB_ARCH_TOKENS="$ARCH_TOKENS" \
            python3 - "$release_json" <<'PY'
import json
import os
import re
import sys

release = json.load(open(sys.argv[1], encoding="utf-8"))
tag = release.get("tag_name") or release.get("name") or "unknown"
assets = release.get("assets") or []
if not assets:
    print(f"NO_ASSETS\t{tag}")
    raise SystemExit(0)

os_tokens = os.environ["REDLINEDB_OS_TOKENS"].split()
arch_tokens = os.environ["REDLINEDB_ARCH_TOKENS"].split()
archive_exts = (
    ".tar.gz", ".tgz", ".tar.xz", ".tar.bz2", ".zip", ".gz", ".xz",
)
ignore_exts = (
    ".sha256", ".sha256sum", ".sig", ".asc", ".pem", ".spdx", ".sbom",
    ".json", ".txt",
)

def split_name(name: str) -> list[str]:
    return [part for part in re.split(r"[^A-Za-z0-9_]+", name.lower()) if part]

best = None
for asset in assets:
    name = asset.get("name") or ""
    url = asset.get("browser_download_url") or ""
    lower = name.lower()
    if not url or lower.endswith(ignore_exts):
        continue
    tokens = split_name(name)
    has_os = any(token in tokens or token in lower for token in os_tokens)
    has_arch = any(token in tokens or token in lower for token in arch_tokens)
    has_redline = "redline" in lower
    has_archive = lower.endswith(archive_exts)
    looks_raw_binary = lower in {"redlinedb", "redline", "redlinedb.exe", "redline.exe"}
    if not (has_os and has_arch and has_redline):
        continue
    score = 0
    score += 20 if "redlinedb" in lower else 0
    score += 10 if has_archive else 0
    score += 15 if looks_raw_binary else 0
    score += max(0, 10 - len(tokens))
    candidate = (score, name, url)
    if best is None or candidate > best:
        best = candidate

if best is None:
    names = ", ".join(asset.get("name") or "<unnamed>" for asset in assets)
    print(f"NO_MATCH\t{tag}\t{names}")
else:
    _, name, url = best
    checksum_name = f"{name}.sha256"
    checksum_url = next(
        (
            asset.get("browser_download_url") or ""
            for asset in assets
            if (asset.get("name") or "") == checksum_name
        ),
        "",
    )
    if not checksum_url:
        print(f"NO_CHECKSUM\t{tag}\t{name}")
    else:
        print(f"OK\t{tag}\t{name}\t{url}\t{checksum_name}\t{checksum_url}")
PY
    )"

    IFS=$'\t' read -r status tag asset_name asset_url checksum_name checksum_url <<< "$selection"
    case "$status" in
        OK) ;;
        NO_CHECKSUM)
            echo "install-redlinedb: upstream RedlineDB release ${tag} has no checksum asset for ${asset_name}." >&2
            echo "install-redlinedb: refusing to install without checksum verification." >&2
            exit 1
            ;;
        NO_ASSETS)
            echo "install-redlinedb: upstream RedlineDB release ${tag} has no binary assets." >&2
            echo "install-redlinedb: publish platform binary release assets before CI/local install can succeed." >&2
            exit 1
            ;;
        NO_MATCH)
            echo "install-redlinedb: upstream RedlineDB release ${tag} has no binary asset for $(uname -s)/$(uname -m)." >&2
            if [ -n "${asset_name:-}" ]; then
                echo "install-redlinedb: available assets: ${asset_name}" >&2
            fi
            echo "install-redlinedb: publish platform binary release assets before CI/local install can succeed." >&2
            exit 1
            ;;
        *)
            echo "install-redlinedb: could not parse RedlineDB release asset metadata" >&2
            exit 1
            ;;
    esac

    asset_path="$tmp/$asset_name"
    checksum_path="$tmp/$checksum_name"
    curl -fL "${auth_args[@]}" -o "$asset_path" "$asset_url"
    curl -fL "${auth_args[@]}" -o "$checksum_path" "$checksum_url"

    python3 - "$asset_path" "$checksum_path" <<'PY'
import hashlib
import pathlib
import re
import sys

asset = pathlib.Path(sys.argv[1])
checksum_file = pathlib.Path(sys.argv[2])
checksum_text = checksum_file.read_text(encoding="utf-8", errors="strict")
match = re.search(r"\b([a-fA-F0-9]{64})\b", checksum_text)
if not match:
    print(
        f"install-redlinedb: checksum file {checksum_file.name} does not contain a SHA256 digest",
        file=sys.stderr,
    )
    raise SystemExit(1)

expected = match.group(1).lower()
actual = hashlib.sha256(asset.read_bytes()).hexdigest()
if actual != expected:
    print(
        f"install-redlinedb: checksum mismatch for {asset.name}: expected {expected}, got {actual}",
        file=sys.stderr,
    )
    raise SystemExit(1)

print(f"install-redlinedb: verified SHA256 for {asset.name}")
PY

    extract_dir="$tmp/extract"
    mkdir -p "$extract_dir"
    case "$asset_name" in
        *.tar.gz|*.tgz) tar -xzf "$asset_path" -C "$extract_dir" ;;
        *.tar.xz) tar -xJf "$asset_path" -C "$extract_dir" ;;
        *.tar.bz2) tar -xjf "$asset_path" -C "$extract_dir" ;;
        *.zip)
            need unzip
            unzip -q "$asset_path" -d "$extract_dir"
            ;;
        *.gz)
            gzip -dc "$asset_path" > "$extract_dir/redlinedb"
            chmod +x "$extract_dir/redlinedb"
            ;;
        *.xz)
            xz -dc "$asset_path" > "$extract_dir/redlinedb"
            chmod +x "$extract_dir/redlinedb"
            ;;
        *)
            cp "$asset_path" "$extract_dir/redlinedb"
            chmod +x "$extract_dir/redlinedb"
            ;;
    esac

    binary=""
    while IFS= read -r candidate; do
        binary="$candidate"
        break
    done < <(
        find "$extract_dir" -type f \( -name redlinedb -o -name redlinedb.exe -o -name redline -o -name redline.exe \) \
            | sort
    )

    if [ -z "$binary" ]; then
        echo "install-redlinedb: release asset ${asset_name} did not contain a redlinedb binary." >&2
        echo "install-redlinedb: publish a platform binary release asset before CI/local install can succeed." >&2
        exit 1
    fi

    chmod +x "$binary"
    mkdir -p "$BIN_DIR"
    install -m 0755 "$binary" "$BIN_DIR/$INSTALL_NAME"

    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *)
            if [ -n "${GITHUB_PATH:-}" ]; then
                printf '%s\n' "$BIN_DIR" >> "$GITHUB_PATH"
            fi
            echo "install-redlinedb: installed to $BIN_DIR; add it to PATH for future shells." >&2
            ;;
    esac
    if [ -n "${GITHUB_ENV:-}" ]; then
        printf 'REDLINEDB_BIN=%s\n' "$BIN_DIR/$INSTALL_NAME" >> "$GITHUB_ENV"
    fi

    if ! "$BIN_DIR/$INSTALL_NAME" --version; then
        echo "install-redlinedb: installed $BIN_DIR/$INSTALL_NAME but --version failed" >&2
        exit 1
    fi

    echo "install-redlinedb: installed ${tag} from ${asset_name} to $BIN_DIR/$INSTALL_NAME"
}

case "$REDLINEDB_INSTALL_MODE" in
    verify) verify_installed_binary ;;
    release) install_from_release ;;
    *)
        echo "install-redlinedb: unsupported REDLINEDB_INSTALL_MODE: $REDLINEDB_INSTALL_MODE" >&2
        echo "install-redlinedb: supported modes: release, verify" >&2
        exit 1
        ;;
esac
