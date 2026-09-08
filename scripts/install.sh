#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
# shellcheck source=scripts/source-build.sh
source "$root/scripts/source-build.sh"
install_dir=${JERYU_INSTALL_DIR:-${HOME:?HOME is required}/.local/bin}
from_source=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --from-source) from_source=true; shift ;;
    --install-dir)
      [[ $# -ge 2 && -n "$2" ]] || { printf '%s\n' '--install-dir needs a path' >&2; exit 2; }
      install_dir=$2; shift 2 ;;
    *) printf 'usage: scripts/install.sh --from-source [--install-dir PATH]\n' >&2; exit 2 ;;
  esac
done
if [[ "$from_source" != true ]]; then
  printf 'Use --from-source. Central signed binary releases are not yet qualified.\n' >&2
  exit 1
fi
artifact="$root/target/release/jeryu"
receipt="$root/target/release/jeryu.source-build"
[[ -f "$receipt" && ! -L "$receipt" && -x "$artifact" && ! -L "$artifact" ]] || {
  printf 'verified source build missing; run ./scripts/build.sh\n' >&2; exit 1;
}
mapfile -t fields < "$receipt"
[[ ${#fields[@]} == 3 && ${fields[0]} == jeryu.source-build/v1 && ${fields[1]} =~ ^[0-9a-f]{64}$ && ${fields[2]} =~ ^[0-9a-f]{64}$ ]] || {
  printf 'invalid source build receipt\n' >&2; exit 1;
}
[[ $(source_digest "$root") == "${fields[1]}" ]] || {
  printf 'source differs from the build; run ./scripts/build.sh\n' >&2; exit 1;
}
mkdir -p -- "$install_dir"
staged=$(mktemp "$install_dir/.jeryu-install.XXXXXXXX")
trap 'rm -f -- "$staged"' EXIT
install -m 0755 -- "$artifact" "$staged"
[[ $(sha256sum -- "$staged" | cut -d ' ' -f 1) == "${fields[2]}" ]] || {
  printf 'source artifact checksum mismatch; installation stopped\n' >&2; exit 1;
}
mv -f -- "$staged" "$install_dir/jeryu"
printf 'Installed %s\n' "$install_dir/jeryu"
