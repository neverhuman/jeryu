#!/usr/bin/env bash
# Owned producers delegate to the single monorepo command.
set -euo pipefail
[[ $# -le 1 ]] || { printf 'usage: proof_evidence.sh [copy-code|migration|independent|full]\n' >&2; exit 2; }
producer=${1:-full}
case $producer in copy-code|migration|independent|full) ;; *) exit 2 ;; esac
component=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
[[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]] || {
  printf 'Standalone auxiliary proof admission awaits the deterministic export and auditor receipt adapter.\n' >&2
  exit 1
}
root=$(env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
  GIT_NO_REPLACE_OBJECTS=1 /usr/bin/git -C "$component" rev-parse --show-toplevel)
[[ $component == "$root/components/jeryu-web" && ! -L "$root/scripts/auxiliary-proofs.sh" ]] || exit 1
exec bash "$root/scripts/auxiliary-proofs.sh" "$producer" jeryu-web
