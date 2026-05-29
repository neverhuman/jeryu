#!/usr/bin/env bash
# scripts/ci/jankurai-staged-install-smoke.sh — verify staged install mode uses
# the repo-local versioned artifact contract without any cross-project fetches.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MANIFEST_PATH="$REPO_ROOT/scripts/jankurai-manifest.json"

version="$(
  python3 - "$MANIFEST_PATH" <<'PY'
import json
import pathlib
import sys

manifest_path = pathlib.Path(sys.argv[1])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
version = manifest.get("version")
if not isinstance(version, str) or not version:
    raise SystemExit("missing version in jankurai manifest")
print(version)
PY
)"

tmp_root="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_root"
}
trap cleanup EXIT

staged_root="$tmp_root/target/jankurai/releases"
staged_binary="$staged_root/$version/jankurai"
install_prefix="$tmp_root/prefix"

mkdir -p "$(dirname "$staged_binary")"
cat >"$staged_binary" <<EOF
#!/usr/bin/env bash
if [ "\${1:-}" = "--version" ]; then
  printf 'jankurai %s\n' "$version"
else
  printf 'jankurai staged smoke binary\n'
fi
EOF
chmod +x "$staged_binary"

JANKURAI_INSTALL_MODE=staged \
JANKURAI_RELEASE_ROOT="$tmp_root/target/jankurai/releases" \
JANKURAI_PREFIX="$install_prefix" \
bash "$REPO_ROOT/scripts/install-jankurai.sh"

"$install_prefix/bin/jankurai" --version | grep -Fx "jankurai $version"
