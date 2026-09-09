#!/usr/bin/env bash
# Verify the real release artifact, installation failures, and installed runtime.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
[[ $# == 0 ]] || { printf 'usage: scripts/test-source-install.sh\n' >&2; exit 2; }
umask 077
bash "$root/tests/source-install-transaction-hostiles.sh"
# shellcheck source=tests/source-install-transaction.sh
source "$root/tests/source-install-transaction.sh"
cleanup() {
  local status=$?
  trap - EXIT
  jeryu_install_finish "$status" || { if [[ $status == 0 ]]; then status=1; fi; }
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
jeryu_install_begin "$root"
: "${scratch:?}" "${artifact:?}"
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
jeryu_install_change artifact-tamper
must_reject artifact-checksum
jeryu_install_restore artifact
jeryu_install_change fixture-create
must_reject changed-source
jeryu_install_change fixture-remove
jeryu_install_change receipt-corrupt
must_reject malformed-receipt
jeryu_install_change receipt-remove
must_reject missing-receipt
jeryu_install_restore receipt

cd "$root"
JERYU_TEST_BINARY="$installed" cargo test --locked -p jeryu-cli --test standalone
if ! jeryu_install_finish 0; then trap - EXIT; exit 1; fi
trap - EXIT
printf 'Source installation, tamper refusal, installed Git/CLI operations and restart persistence passed.\n'
