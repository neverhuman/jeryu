#!/usr/bin/env bash
# Verify the real release artifact, installation failures, and installed runtime.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
[[ $# == 0 ]] || { printf 'usage: scripts/test-source-install.sh\n' >&2; exit 2; }
umask 077
scratch=$(mktemp -d)
fixture="$root/.source-install-fixture-$$"
artifact="$root/target/release/jeryu"
receipt="$root/target/release/jeryu.source-build"
[[ ! -e "$fixture" && ! -L "$fixture" ]] || exit 1
cleanup() {
  if [[ -f "$scratch/artifact" ]]; then install -m 0755 "$scratch/artifact" "$artifact"; fi
  if [[ -f "$scratch/receipt" ]]; then install -m 0600 "$scratch/receipt" "$receipt"; fi
  rm -f -- "$fixture"
  rm -rf -- "$scratch"
}
trap cleanup EXIT
cp -- "$artifact" "$scratch/artifact"
cp -- "$receipt" "$scratch/receipt"
mkdir "$scratch/home"
cd "$scratch"
env -i PATH=/usr/bin:/bin HOME="$scratch/home" GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
  bash "$root/scripts/install.sh" --from-source
installed="$scratch/home/.local/bin/jeryu"
cmp -- "$artifact" "$installed"
"$installed" serve --help >/dev/null
JERYU_INSTALL_DIR="$scratch/environment-bin" bash "$root/scripts/install.sh" --from-source
cmp -- "$artifact" "$scratch/environment-bin/jeryu"
JERYU_INSTALL_DIR="$scratch/unused" bash "$root/scripts/install.sh" --from-source --install-dir "$scratch/explicit-bin"
cmp -- "$artifact" "$scratch/explicit-bin/jeryu"
[[ ! -e "$scratch/unused" ]]

must_reject() {
  if bash "$root/scripts/install.sh" --from-source --install-dir "$scratch/explicit-bin" > "$scratch/rejection.log" 2>&1; then
    printf 'invalid source installation unexpectedly succeeded: %s\n' "$1" >&2; exit 1;
  fi
  cmp -- "$scratch/artifact" "$scratch/explicit-bin/jeryu"
}
printf 'tampered\n' >> "$artifact"
must_reject artifact-checksum
install -m 0755 "$scratch/artifact" "$artifact"
printf 'source changed\n' > "$fixture"
must_reject changed-source
rm -- "$fixture"
printf 'invalid receipt\n' > "$receipt"
must_reject malformed-receipt
rm -- "$receipt"
must_reject missing-receipt
install -m 0600 "$scratch/receipt" "$receipt"

cd "$root"
JERYU_TEST_BINARY="$installed" cargo test --locked -p jeryu-cli --test standalone
printf 'Source installation, tamper refusal, installed Git/CLI operations and restart persistence passed.\n'
