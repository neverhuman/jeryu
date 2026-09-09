#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
if (exec 3<>/dev/tcp/127.0.0.1/18788) 2>/dev/null; then
  printf 'performance fixture port 18788 is already occupied\n' >&2; exit 1;
fi
umask 077
mkdir -p target/ci
JERYU_BROWSER_PASSWORD=$(openssl rand -hex 32)
export JERYU_BROWSER_PASSWORD
JERYU_BROWSER_BIND=127.0.0.1:18788 bash scripts/ci-browser-server.sh > target/ci/perf-server.log 2>&1 &
server_pid=$!
trap 'kill "$server_pid" 2>/dev/null || true; wait "$server_pid" 2>/dev/null || true' EXIT
ready=false
for ((attempt = 0; attempt < 240; attempt++)); do
  kill -0 "$server_pid" 2>/dev/null || { printf 'performance fixture server stopped\n' >&2; exit 1; }
  if curl --fail --silent http://127.0.0.1:18788/health >/dev/null; then ready=true; break; fi
  sleep 0.25
done
[[ $ready == true ]] || { printf 'performance fixture server did not become ready\n' >&2; exit 1; }
CHROME_PATH=$(node -e 'process.stdout.write(require("playwright").chromium.executablePath())')
export CHROME_PATH
JERYU_LIGHTHOUSE_URL=http://127.0.0.1:18788/ npm --workspace @jeryu/web run perf
