#!/usr/bin/env bash
# Own one disposable server for the existing live browser smoke tests.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
umask 077
data=$(mktemp -d)
server_pid=
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf -- "$data"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# Playwright supplies an ephemeral password through the environment. Nothing is
# printed or placed in command arguments, and the real auth gate stays enabled.
JERYU_BOOTSTRAP_ADMIN_PASSWORD="${JERYU_BROWSER_PASSWORD:?browser fixture password is required}" \
  JERYU_WEB_TRUST_LOCAL=0 "$root/target/debug/jeryu" serve \
  --bind "${JERYU_BROWSER_BIND:-127.0.0.1:18787}" --data-dir "$data" &
server_pid=$!
wait "$server_pid"
