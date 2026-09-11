#!/usr/bin/env bash
# Install the pinned 1.6.11 GitHub Release binary and verify its SHA.
# Downloads and extracts outside the git checkout so audit sees a clean tree.
set -euo pipefail
tag="${JANKURAI_TAG:-v1.6.11-deadlang-precision-split.3}"
expected="${JANKURAI_SHA256:-9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c}"
version="${JANKURAI_VERSION:-jankurai 1.6.11}"
asset="jankurai-1.6.11-deadlang-precision-split.3-x86_64-unknown-linux-gnu.tar.gz"
work="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/jankurai-release-$$"
mkdir -m 700 -p "$work"
cleanup() { rm -rf -- "$work"; }
trap cleanup EXIT
cd "$work"
curl -fsSL -o "$asset" \
  "https://github.com/neverhuman/jankurai/releases/download/${tag}/${asset}"
curl -fsSL -o "${asset}.sha256" \
  "https://github.com/neverhuman/jankurai/releases/download/${tag}/${asset}.sha256"
sha256sum -c "${asset}.sha256"
tar -xzf "$asset"
bin="$(find "$work" -name jankurai -type f -perm -u+x | head -1)"
[[ -n "$bin" ]]
actual="$(sha256sum "$bin" | awk '{print $1}')"
test "$actual" = "$expected"
test "$("$bin" --version)" = "$version"
if [[ "${EUID}" -eq 0 ]]; then
  install -m 0755 "$bin" /usr/local/bin/jankurai
else
  sudo install -m 0755 "$bin" /usr/local/bin/jankurai
fi
command -v jankurai
jankurai --version
