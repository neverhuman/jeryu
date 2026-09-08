#!/usr/bin/env bash
# Public, pinned CI prerequisites. The governed Jankurai receipt is separate.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
[[ $# == 0 || ( $# == 1 && $1 == --binary-tools-only ) ]] || {
  printf 'usage: scripts/bootstrap-ci-tools.sh [--binary-tools-only]\n' >&2; exit 2;
}
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || {
  printf 'CI tool bootstrap currently supports Linux x86_64\n' >&2; exit 1;
}
umask 077
tool_root="$root/target/ci-tools"
mkdir -p "$tool_root/bin"
[[ ! -L "$root/target" && ! -L "$tool_root" && ! -L "$tool_root/bin" ]] || {
  printf 'CI tools require physical directories\n' >&2; exit 1;
}
scratch=$(mktemp -d "$tool_root/bootstrap.XXXXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
while read -r name version member digest url extra; do
  [[ -n "$name" && $name != \#* ]] || continue
  [[ -z "${extra:-}" && $name =~ ^[a-z][a-z-]*$ && $digest =~ ^[0-9a-f]{64}$ && $url == https://github.com/* ]] || {
    printf 'invalid CI artifact lock row\n' >&2; exit 1;
  }
  curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
    --tlsv1.2 "$url" --output "$scratch/archive"
  [[ $(sha256sum "$scratch/archive" | cut -d ' ' -f 1) == "$digest" ]] || {
    printf 'CI artifact checksum mismatch: %s %s\n' "$name" "$version" >&2; exit 1;
  }
  if [[ $member == - ]]; then
    mv "$scratch/archive" "$scratch/binary"
  else
    [[ $member == "$name" ]] || { printf 'invalid archive member\n' >&2; exit 1; }
    # Stream only the pinned member; archive paths never become filesystem paths.
    tar -xOzf "$scratch/archive" -- "$member" > "$scratch/binary"
  fi
  [[ -s "$scratch/binary" ]] || { printf 'empty CI tool artifact\n' >&2; exit 1; }
  chmod 0755 "$scratch/binary"
  mv "$scratch/binary" "$tool_root/bin/$name"
  printf 'Verified %s %s\n' "$name" "$version"
done < "$root/ci/tools.lock.tsv"
if [[ ${1:-} != --binary-tools-only ]]; then
  while read -r crate version extra; do
    [[ -n "$crate" && $crate != \#* ]] || continue
    [[ -z "${extra:-}" && $crate =~ ^[a-z][a-z-]*$ && $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
      printf 'invalid Cargo tool lock row\n' >&2; exit 1;
    }
    cargo install --locked --registry crates-io --version "$version" --root "$tool_root" "$crate"
  done < "$root/ci/cargo-tools.lock.tsv"
fi
printf 'CI tools installed in %s\n' "$tool_root/bin"
