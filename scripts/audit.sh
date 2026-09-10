#!/usr/bin/env bash
# The Rust census owns selection, report admission and all-result aggregation.
set -euo pipefail
umask 077
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
parent=${JERYU_AUDIT_OUTPUT_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/jeryu/audits}
mkdir -p -- "$parent"
parent=$(cd -- "$parent" && pwd -P)
case "$parent/" in "$root/"*) printf 'audit output must be outside source\n' >&2; exit 2 ;; esac
attempt=$(mktemp -d "$parent/attempt.XXXXXXXX")
args=(--inventory "$root/agent/audit-repositories.json" --out "$attempt/census")
if (
  source "$root/scripts/bootstrap-jankurai.sh"
  bootstrap_public_jankurai || exit "$?"
  printf '%s\n%s\n' "$JERYU_GOVERNED_JANKURAI_BIN" "$JERYU_JANKURAI_RECEIPT" >"$attempt/selection"
) >"$attempt/bootstrap.log" 2>&1; then
  mapfile -t selection <"$attempt/selection"
  [[ ${#selection[@]} == 2 ]] || exit 1
  args+=(--auditor "${selection[0]}" --receipt "${selection[1]}")
else
  printf 'Auditor bootstrap failed; census will record every unexecuted scope. Private log: %s\n' "$attempt/bootstrap.log" >&2
fi
# A local SHA is policy comparison input, not protected predecessor authentication.
if [[ -n ${JERYU_AUDIT_GOVERNING_COMMIT:-} ]]; then
  args+=(--governing-commit "$JERYU_AUDIT_GOVERNING_COMMIT")
fi
cargo run --locked -p jeryu-split-tool --bin jeryu-split -- audit-census "${args[@]}"
