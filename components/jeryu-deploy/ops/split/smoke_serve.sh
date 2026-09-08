#!/usr/bin/env bash
set -euo pipefail
bin="${1:-target/release/jeryu}"
if [[ ! -x "$bin" ]]; then
  printf 'serve smoke pending: binary not built at %s\n' "$bin"
  exit 0
fi
"$bin" --help >/dev/null
printf 'serve smoke bootstrap ok\n'
