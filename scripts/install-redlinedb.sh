#!/usr/bin/env bash
# Install the required RedlineDB CLI/native binary from the checked-in pinned
# release manifest. The manifest keeps the exact release tag, asset name,
# download URL, and SHA256 in-repo, so installs do not depend on GitHub API
# lookups or auth.
#
# Use REDLINEDB_INSTALL_MODE=verify only when intentionally checking an
# already-installed binary without network access.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST_PATH="${REDLINEDB_MANIFEST:-$SCRIPT_DIR/redlinedb-manifest.json}"
REDLINEDB_INSTALL_MODE="${REDLINEDB_INSTALL_MODE:-release}"
PREFIX="${REDLINEDB_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
INSTALL_NAME="${REDLINEDB_INSTALL_NAME:-redlinedb}"
REDLINEDB_BIN="${REDLINEDB_BIN:-$BIN_DIR/$INSTALL_NAME}"

need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "install-redlinedb: missing required tool: $1" >&2
        exit 1
    }
}

platform_key() {
    local os arch

    case "$(uname -s)" in
        Linux) os="linux" ;;
        Darwin) os="macos" ;;
        *)
            echo "install-redlinedb: unsupported OS: $(uname -s)" >&2
            exit 1
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="arm64" ;;
        *)
            echo "install-redlinedb: unsupported architecture: $(uname -m)" >&2
            exit 1
            ;;
    esac

    printf '%s-%s\n' "$os" "$arch"
}

manifest_entry() {
    python3 - "$MANIFEST_PATH" "$1" <<'PY'
import json
import pathlib
import sys

manifest_path = pathlib.Path(sys.argv[1])
platform_key = sys.argv[2]

try:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
except FileNotFoundError:
    print(f"install-redlinedb: missing release manifest: {manifest_path}", file=sys.stderr)
    raise SystemExit(1)
except json.JSONDecodeError as exc:
    print(f"install-redlinedb: invalid release manifest {manifest_path}: {exc}", file=sys.stderr)
    raise SystemExit(1)

release_tag = manifest.get("release_tag")
assets = manifest.get("assets")
if not isinstance(release_tag, str) or not release_tag:
    print(
        f"install-redlinedb: release manifest {manifest_path} is missing release_tag",
        file=sys.stderr,
    )
    raise SystemExit(1)
if not isinstance(assets, dict):
    print(
        f"install-redlinedb: release manifest {manifest_path} must map assets by platform",
        file=sys.stderr,
    )
    raise SystemExit(1)

entry = assets.get(platform_key)
if not isinstance(entry, dict):
    supported = ", ".join(sorted(assets)) or "<none>"
    print(
        f"install-redlinedb: release manifest {manifest_path} has no asset for platform "
        f"{platform_key}; supported platforms: {supported}",
        file=sys.stderr,
    )
    raise SystemExit(1)

for field in ("asset_name", "download_url", "sha256"):
    value = entry.get(field)
    if not isinstance(value, str) or not value:
        print(
            f"install-redlinedb: release manifest {manifest_path} entry for {platform_key} "
            f"is missing {field}",
            file=sys.stderr,
        )
        raise SystemExit(1)

print(f"OK\t{release_tag}\t{entry['asset_name']}\t{entry['download_url']}\t{entry['sha256']}")
PY
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

    local platform selection status release_tag asset_name asset_url asset_sha256
    local asset_path extract_dir binary tmp
    platform="$(platform_key)"

    tmp="$(mktemp -d)"
    cleanup() {
        if [ -n "${tmp:-}" ]; then
            rm -rf "$tmp"
        fi
    }
    trap cleanup EXIT

    selection="$(manifest_entry "$platform")"
    IFS=$'\t' read -r status release_tag asset_name asset_url asset_sha256 <<< "$selection"
    if [ "$status" != "OK" ]; then
        echo "install-redlinedb: could not parse RedlineDB release manifest" >&2
        exit 1
    fi

    asset_path="$tmp/$asset_name"
    curl -fsSL -o "$asset_path" "$asset_url"

    python3 - "$asset_path" "$asset_sha256" <<'PY'
import hashlib
import pathlib
import sys

asset = pathlib.Path(sys.argv[1])
expected = sys.argv[2].strip().lower()
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

    echo "install-redlinedb: installed ${release_tag} from ${asset_name} to $BIN_DIR/$INSTALL_NAME"
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
