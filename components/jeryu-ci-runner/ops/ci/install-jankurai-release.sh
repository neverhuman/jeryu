#!/usr/bin/env bash
# Install the pinned 1.6.11 GitHub Release binary and verify its SHA.
# Downloads and extracts outside the git checkout so audit sees a clean tree.
# Idempotent and flock-serialized: parallel GHA tests must not race GNU
# install's O_EXCL create of /usr/local/bin/jankurai.
set -euo pipefail
tag="${JANKURAI_TAG:-v1.6.11-deadlang-precision-split.3}"
expected="${JANKURAI_SHA256:-9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c}"
version="${JANKURAI_VERSION:-jankurai 1.6.11}"
asset="jankurai-1.6.11-deadlang-precision-split.3-x86_64-unknown-linux-gnu.tar.gz"
dest=/usr/local/bin/jankurai

already_verified() {
  [[ -f "$dest" && ! -L "$dest" && -x "$dest" ]] || return 1
  [[ "$(sha256sum "$dest" | awk '{print $1}')" == "$expected" ]] || return 1
  [[ "$("$dest" --version)" == "$version" ]]
}

if already_verified; then
  command -v jankurai
  jankurai --version
  exit 0
fi

lock_dir="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
mkdir -p "$lock_dir"
lock="$lock_dir/jankurai-release-install.lock"
exec 9>"$lock"
flock 9
if already_verified; then
  command -v jankurai
  jankurai --version
  exit 0
fi

work="${lock_dir}/jankurai-release-$$"
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
staging="${dest}.new.$$"
if [[ "${EUID}" -eq 0 ]]; then
  install -m 0755 "$bin" "$staging"
  mv -f -- "$staging" "$dest"
else
  sudo install -m 0755 "$bin" "$staging"
  sudo mv -f -- "$staging" "$dest"
fi
command -v jankurai
jankurai --version
