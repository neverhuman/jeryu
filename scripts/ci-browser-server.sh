#!/usr/bin/env bash
# Own and retain one private server fixture for live browser smoke tests.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
umask 077
# shellcheck source=tests/scratch.sh
source "$root/tests/scratch.sh"
data=$(mktemp -d -t jeryu-browser.XXXXXXXX)
jeryu_record_test_scratch "$data" || {
  printf 'Retaining unadmitted browser fixture: %q\n' "$data" >&2
  exit 1
}
server_pid=
cleanup() {
  local status=$?
  trap - EXIT
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  # Playwright stops this wrapper after both passing and failing tests.
  # A server exit status cannot authorize deletion of its fixture.
  printf 'Browser fixture retained; Playwright result unknown; wrapper exit=%s; path=%q; identity=%s\n' \
    "$status" "$data" "$jeryu_test_scratch_identity" >&2
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
binary_sha256=$(sha256sum -- "$root/target/debug/jeryu" | cut -d ' ' -f 1)
(
  set -o noclobber
  printf 'schema_version=1\nscratch_identity=%s\nserver_binary_sha256=%s\nplaywright_result=unknown\n' \
    "$jeryu_test_scratch_identity" "$binary_sha256" > "$data/fixture-origin.txt"
)
printf 'Browser fixture allocated: path=%q; identity=%s\n' \
  "$data" "$jeryu_test_scratch_identity" >&2
# Playwright supplies an ephemeral password through the environment. Nothing is
# printed or placed in command arguments, and the real auth gate stays enabled.
JERYU_BOOTSTRAP_ADMIN_PASSWORD="${JERYU_BROWSER_PASSWORD:?browser fixture password is required}" \
  JERYU_WEB_TRUST_LOCAL=0 "$root/target/debug/jeryu" serve \
  --bind "${JERYU_BROWSER_BIND:-127.0.0.1:18787}" --data-dir "$data" \
  > "$data/server.log" 2>&1 &
server_pid=$!
status=0
wait "$server_pid" || status=$?
server_pid=
exit "$status"
