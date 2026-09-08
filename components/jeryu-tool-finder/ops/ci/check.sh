#!/usr/bin/env bash
# Structure check: the Rust finder builds clean (fmt/clippy/test) and the
# shell entrypoints are valid. The dossier fixture selftest rides cargo test.
set -euo pipefail
source ops/ci/lib.sh
require_tool jq
require_tool shellcheck

# Shell entrypoints and their dispatch test must parse.
for script in scripts/*.sh tools/*.sh tests/*.sh ops/*.sh ops/ci/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
mapfile -t shell_scripts < <(git ls-files '*.sh' | sort)
shellcheck -S warning "${shell_scripts[@]}"

bash tests/ci_local_dispatch_test.sh
bash tests/source_authority_test.sh
bash tests/score_auditor_test.sh
bash tests/score_auditor_git_env_test.sh

cargo fmt --check
metadata="$(cargo metadata --locked --offline --format-version 1)"
expected_intelligence_source='git+http://127.0.0.1:8787/git/jeryu/jeryu-intelligence.git?tag=jeryu-intelligence-v5.0.0-split.1#6fb845c594c3e5e9ffea8047d8a3f814fa9ba4da'
jq -e --arg source "$expected_intelligence_source" '
  [.packages[]
    | select(.name == "jeryu-codegraph" or .name == "jeryu-rustjet")
    | {name, source: (.source // "")}]
  | sort_by(.name)
  == [
    {name: "jeryu-codegraph", source: $source},
    {name: "jeryu-rustjet", source: $source}
  ]
' <<<"$metadata" >/dev/null
cargo clippy --locked --offline --all-targets -- -D warnings
cargo test --locked --offline

printf 'check ok: %s\n' "$(pwd)"
