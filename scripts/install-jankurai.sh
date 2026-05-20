#!/usr/bin/env bash
# Install the required Jankurai CLI from the checked-in pinned release
# manifest. This keeps local proof lanes and CI on the same release binary
# without GitHub API lookups or local-path/cargo-install drift.
#
# Use JANKURAI_INSTALL_MODE=verify only when intentionally checking an
# already-installed binary without network access.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST_PATH="${JANKURAI_MANIFEST:-$SCRIPT_DIR/jankurai-manifest.json}"
JANKURAI_INSTALL_MODE="${JANKURAI_INSTALL_MODE:-release}"
PREFIX="${JANKURAI_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
INSTALL_NAME="${JANKURAI_INSTALL_NAME:-jankurai}"
JANKURAI_BIN="${JANKURAI_BIN:-$BIN_DIR/$INSTALL_NAME}"
BUILD_MANIFEST_DIR="/home/runner/work/jankurai/jankurai/crates/jankurai"
RUNTIME_MANIFEST_DIR="/tmp/jankurai-v1.5.1-runtime/aaaaaa/crates/jankurai"
RUNTIME_SCHEMA_DIR="/tmp/jankurai-v1.5.1-runtime/aaaaaa/schemas"

need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "install-jankurai: missing required tool: $1" >&2
        exit 1
    }
}

platform_key() {
    local os arch

    case "$(uname -s)" in
        Linux) os="linux" ;;
        Darwin) os="macos" ;;
        *)
            echo "install-jankurai: unsupported OS: $(uname -s)" >&2
            exit 1
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="arm64" ;;
        *)
            echo "install-jankurai: unsupported architecture: $(uname -m)" >&2
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
    print(f"install-jankurai: missing release manifest: {manifest_path}", file=sys.stderr)
    raise SystemExit(1)
except json.JSONDecodeError as exc:
    print(f"install-jankurai: invalid release manifest {manifest_path}: {exc}", file=sys.stderr)
    raise SystemExit(1)

release_tag = manifest.get("release_tag")
version = manifest.get("version")
assets = manifest.get("assets")
if not isinstance(release_tag, str) or not release_tag:
    print(f"install-jankurai: release manifest {manifest_path} is missing release_tag", file=sys.stderr)
    raise SystemExit(1)
if not isinstance(version, str) or not version:
    print(f"install-jankurai: release manifest {manifest_path} is missing version", file=sys.stderr)
    raise SystemExit(1)
if not isinstance(assets, dict):
    print(
        f"install-jankurai: release manifest {manifest_path} must map assets by platform",
        file=sys.stderr,
    )
    raise SystemExit(1)

entry = assets.get(platform_key)
if not isinstance(entry, dict):
    supported = ", ".join(sorted(assets)) or "<none>"
    print(
        f"install-jankurai: release manifest {manifest_path} has no asset for platform "
        f"{platform_key}; supported platforms: {supported}",
        file=sys.stderr,
    )
    raise SystemExit(1)

for field in ("asset_name", "download_url", "sha256"):
    value = entry.get(field)
    if not isinstance(value, str) or not value:
        print(
            f"install-jankurai: release manifest {manifest_path} entry for {platform_key} "
            f"is missing {field}",
            file=sys.stderr,
        )
        raise SystemExit(1)

print(
    f"OK\t{release_tag}\t{version}\t{entry['asset_name']}\t"
    f"{entry['download_url']}\t{entry['sha256']}"
)
PY
}

manifest_source() {
    python3 - "$MANIFEST_PATH" <<'PY'
import json
import pathlib
import sys

manifest_path = pathlib.Path(sys.argv[1])
try:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
except FileNotFoundError:
    print(f"install-jankurai: missing release manifest: {manifest_path}", file=sys.stderr)
    raise SystemExit(1)
except json.JSONDecodeError as exc:
    print(f"install-jankurai: invalid release manifest {manifest_path}: {exc}", file=sys.stderr)
    raise SystemExit(1)

url = manifest.get("source_tarball_url")
sha256 = manifest.get("source_tarball_sha256")
if not isinstance(url, str) or not url:
    print(f"install-jankurai: release manifest {manifest_path} is missing source_tarball_url", file=sys.stderr)
    raise SystemExit(1)
if not isinstance(sha256, str) or not sha256:
    print(f"install-jankurai: release manifest {manifest_path} is missing source_tarball_sha256", file=sys.stderr)
    raise SystemExit(1)

print(f"OK\t{url}\t{sha256}")
PY
}

verify_sha256() {
    local label="$1"
    local path="$2"
    local expected="$3"
    python3 - "$label" "$path" "$expected" <<'PY'
import hashlib
import pathlib
import sys

label = sys.argv[1]
asset = pathlib.Path(sys.argv[2])
expected = sys.argv[3].strip().lower()
actual = hashlib.sha256(asset.read_bytes()).hexdigest()
if actual != expected:
    print(
        f"install-jankurai: checksum mismatch for {label}: expected {expected}, got {actual}",
        file=sys.stderr,
    )
    raise SystemExit(1)

print(f"install-jankurai: verified SHA256 for {label}")
PY
}

install_runtime_schemas() {
    local tmp="$1"
    local source_selection source_status source_url source_sha256 source_path extract_dir schema_dir

    source_selection="$(manifest_source)"
    IFS=$'\t' read -r source_status source_url source_sha256 <<< "$source_selection"
    if [ "$source_status" != "OK" ]; then
        echo "install-jankurai: could not parse Jankurai source manifest" >&2
        exit 1
    fi

    source_path="$tmp/jankurai-source.tar.gz"
    curl -fsSL -o "$source_path" "$source_url"
    verify_sha256 "jankurai source tarball" "$source_path" "$source_sha256"

    extract_dir="$tmp/source"
    mkdir -p "$extract_dir"
    tar -xzf "$source_path" -C "$extract_dir"

    schema_dir=""
    while IFS= read -r candidate; do
        if [ -f "$candidate/repo-score.schema.json" ]; then
            schema_dir="$candidate"
            break
        fi
    done < <(find "$extract_dir" -type d -name schemas | sort)

    if [ -z "$schema_dir" ]; then
        echo "install-jankurai: source tarball did not contain schemas/repo-score.schema.json" >&2
        exit 1
    fi

    mkdir -p "$RUNTIME_MANIFEST_DIR"
    rm -rf "$RUNTIME_SCHEMA_DIR"
    mkdir -p "$RUNTIME_SCHEMA_DIR"
    cp -R "$schema_dir/." "$RUNTIME_SCHEMA_DIR/"
}

patch_runtime_schema_root() {
    local binary="$1"
    python3 - "$binary" "$BUILD_MANIFEST_DIR" "$RUNTIME_MANIFEST_DIR" <<'PY'
import pathlib
import sys

binary = pathlib.Path(sys.argv[1])
needle = sys.argv[2].encode()
replacement = sys.argv[3].encode()
if len(needle) != len(replacement):
    print(
        f"install-jankurai: runtime schema patch path length mismatch: "
        f"{len(needle)} != {len(replacement)}",
        file=sys.stderr,
    )
    raise SystemExit(1)

data = binary.read_bytes()
count = data.count(needle)
if count == 0:
    if replacement in data:
        print(f"install-jankurai: runtime schema root already patched in {binary}")
        raise SystemExit(0)
    print(f"install-jankurai: build schema root not found in {binary}", file=sys.stderr)
    raise SystemExit(1)
if count > 8:
    print(
        f"install-jankurai: refusing to patch {binary}; expected a small schema-root count, found {count}",
        file=sys.stderr,
    )
    raise SystemExit(1)

binary.write_bytes(data.replace(needle, replacement))
print(f"install-jankurai: patched {count} runtime schema root references in {binary}")
PY
}

verify_installed_binary() {
    if [ ! -e "$JANKURAI_BIN" ]; then
        echo "install-jankurai: required binary is missing: $JANKURAI_BIN" >&2
        echo "install-jankurai: install or symlink Jankurai at $JANKURAI_BIN" >&2
        exit 1
    fi

    if [ ! -x "$JANKURAI_BIN" ]; then
        echo "install-jankurai: required binary is not executable: $JANKURAI_BIN" >&2
        echo "install-jankurai: install or symlink Jankurai at $JANKURAI_BIN" >&2
        exit 1
    fi

    local version_output expected_version
    expected_version="$(manifest_entry "$(platform_key)" | cut -f3)"
    if ! version_output="$("$JANKURAI_BIN" --version 2>&1)"; then
        echo "install-jankurai: $JANKURAI_BIN --version failed" >&2
        printf '%s\n' "$version_output" >&2
        echo "install-jankurai: install or symlink Jankurai at $JANKURAI_BIN" >&2
        exit 1
    fi

    case "$version_output" in
        *" $expected_version"|*" $expected_version "*) ;;
        *)
            echo "install-jankurai: expected version $expected_version, got: $version_output" >&2
            exit 1
            ;;
    esac

    printf 'install-jankurai: verified %s (%s)\n' "$JANKURAI_BIN" "$version_output"
}

install_from_release() {
    need curl
    need python3

    local platform selection status release_tag version asset_name asset_url asset_sha256
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
    IFS=$'\t' read -r status release_tag version asset_name asset_url asset_sha256 <<< "$selection"
    if [ "$status" != "OK" ]; then
        echo "install-jankurai: could not parse Jankurai release manifest" >&2
        exit 1
    fi

    asset_path="$tmp/$asset_name"
    curl -fsSL -o "$asset_path" "$asset_url"

    verify_sha256 "$asset_name" "$asset_path" "$asset_sha256"

    extract_dir="$tmp/extract"
    mkdir -p "$extract_dir"
    case "$asset_name" in
        *.tar.gz|*.tgz) tar -xzf "$asset_path" -C "$extract_dir" ;;
        *)
            echo "install-jankurai: unsupported asset extension: $asset_name" >&2
            exit 1
            ;;
    esac

    binary=""
    while IFS= read -r candidate; do
        binary="$candidate"
        break
    done < <(
        find "$extract_dir" -type f \( -name jankurai -o -name jankurai.exe \) | sort
    )

    if [ -z "$binary" ]; then
        echo "install-jankurai: release asset ${asset_name} did not contain a jankurai binary." >&2
        echo "install-jankurai: publish a platform binary release asset before CI/local install can succeed." >&2
        exit 1
    fi

    chmod +x "$binary"
    install_runtime_schemas "$tmp"
    patch_runtime_schema_root "$binary"
    mkdir -p "$BIN_DIR"
    install -m 0755 "$binary" "$BIN_DIR/$INSTALL_NAME"

    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *)
            if [ -n "${GITHUB_PATH:-}" ]; then
                printf '%s\n' "$BIN_DIR" >> "$GITHUB_PATH"
            fi
            ;;
    esac

    verify_installed_binary
    printf 'install-jankurai: installed %s from %s (%s)\n' "$JANKURAI_BIN" "$release_tag" "$asset_name"
}

case "$JANKURAI_INSTALL_MODE" in
    release) install_from_release ;;
    verify) verify_installed_binary ;;
    *)
        echo "install-jankurai: unsupported JANKURAI_INSTALL_MODE=$JANKURAI_INSTALL_MODE" >&2
        echo "install-jankurai: expected release or verify" >&2
        exit 1
        ;;
esac
