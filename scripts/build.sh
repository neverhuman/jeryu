#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
# shellcheck source=scripts/source-build.sh
source "$root/scripts/source-build.sh"
for tool in git cargo rustc node npm cc pkg-config sha256sum; do
  command -v "$tool" >/dev/null || { printf 'missing prerequisite: %s\n' "$tool" >&2; exit 1; }
done
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || {
  printf 'this source installer currently supports Linux x86_64\n' >&2; exit 1;
}
node -e 'const [major, minor] = process.versions.node.split(".").map(Number); if (!(major >= 24 || (major === 22 && minor >= 19))) process.exit(1)' || {
  printf 'Node.js 22.19+ on the 22.x line, or Node.js 24+, is required\n' >&2; exit 1;
}
pkg-config --exists openssl || { printf 'OpenSSL development headers are required\n' >&2; exit 1; }
[[ $# == 0 ]] || { printf 'usage: scripts/build.sh\n' >&2; exit 2; }
source_sha=$(source_digest "$root")
npm ci
npm run build
export JERYU_WEB_DIST="$root/components/jeryu-web/apps/web/dist"
export JERYU_REQUIRE_WEB=1
[[ -s "$JERYU_WEB_DIST/index.html" ]] || { printf 'web build did not produce index.html\n' >&2; exit 1; }
cargo build --locked --release --target-dir "$root/target" -p jeryu-cli --bin jeryu
[[ $(source_digest "$root") == "$source_sha" ]] || {
  printf 'source changed during build; rebuild before installing\n' >&2; exit 1;
}
artifact_sha=$(sha256sum target/release/jeryu | cut -d ' ' -f 1)
receipt=$(mktemp "$root/target/release/source-build.XXXXXXXX")
trap 'rm -f -- "$receipt"' EXIT
printf 'jeryu.source-build/v1\n%s\n%s\n' "$source_sha" "$artifact_sha" > "$receipt"
mv -- "$receipt" target/release/jeryu.source-build
printf 'Built target/release/jeryu. Install with ./scripts/install.sh --from-source\n'
