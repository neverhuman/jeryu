#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$repo_root"

contract="$repo_root/contracts/cli-help.txt"
if [[ ! -f "$contract" || -L "$contract" ]] ||
   [[ "$(stat -c '%h' "$contract")" -ne 1 ]]; then
  printf 'contract-drift failed: CLI help snapshot is not a regular single-link file\n' >&2
  exit 1
fi

cargo test --locked --offline --test cli_contract
printf 'contract-drift ok: contracts/cli-help.txt\n'
