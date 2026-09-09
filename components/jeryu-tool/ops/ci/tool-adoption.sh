#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
cd "$REPO_ROOT"
require_jankurai
for tool in git jq; do
  require_tool "$tool"
done

[[ -z "$(git status --porcelain=v1 --untracked-files=all)" ]] || {
  printf 'tool adoption requires a clean committed source tree\n' >&2
  exit 1
}
proof_base="${JAIN_CONTRACT_BASE_REF:-origin/main}"
git rev-parse --verify "$proof_base^{commit}" >/dev/null
git merge-base --is-ancestor "$proof_base" HEAD
for path in target target/jankurai target/jankurai/proofbind \
  target/jankurai/proofmark target/jankurai/rust target/jankurai/security; do
  if [[ -L "$path" || ( -e "$path" && ! -d "$path" ) ]]; then
    printf 'tool-adoption output is not a physical directory: %s\n' "$path" >&2
    exit 1
  fi
  [[ -d "$path" ]] || mkdir -m 0750 -- "$path"
  [[ "$(realpath -e -- "$path")" == "$REPO_ROOT/$path" ]]
done
for output in target/jankurai/accepted-baseline.json \
  target/jankurai/accepted-baseline.md target/jankurai/repo-score.json \
  target/jankurai/repo-score.md target/jankurai/repair-queue.jsonl \
  target/jankurai/language-bad-behavior.log; do
  [[ ! -e "$output" || ( -f "$output" && ! -L "$output" ) ]]
  if [[ -e "$output" ]]; then
    [[ "$(stat -Lc '%u:%h' -- "$output")" == "$EUID:1" ]]
  fi
  rm -f -- "$output"
done

# Generate and hostile-validate the exact changed-surface route first.
bash ops/ci/proof-routing.sh

if [[ "$proof_base" == origin/main ]]; then
  jankurai proofbind verify . --changed-from origin/main --mode advisory --out target/jankurai/proofbind/surface-witness.json --obligations-out target/jankurai/proofbind/obligations.json --md target/jankurai/proofbind/proofbind.md
else
  jankurai proofbind verify . --changed-from "$proof_base" --mode advisory \
    --out target/jankurai/proofbind/surface-witness.json \
    --obligations-out target/jankurai/proofbind/obligations.json \
    --md target/jankurai/proofbind/proofbind.md
fi
jankurai proofmark rust . --obligations target/jankurai/proofbind/obligations.json --mode advisory
jankurai rust witness build . --out target/jankurai/rust/witness-graph.json
jankurai copy-code . --json target/jankurai/copy-code.json --md target/jankurai/copy-code.md

# The governed security wrapper runs real tools; this receipt binds the exact
# head and command outcomes before any audit consumes security evidence.
jankurai security run . --out target/jankurai/security/evidence.json --strict --profile ci --script tools/security-lane.sh
jq -e --arg head "$(git rev-parse HEAD)" '
  .schema_version == "1.0.0" and .git_head == $head and .lane == "security" and
  .exit_code == 0 and .wrapper.path == "tools/security-lane.sh" and
  ([.commands[] | select(.status != "ran" or .exit_code != 0 or .blocking == true)] | length) == 0
' target/jankurai/security/evidence.json >/dev/null

bash ops/ci/coverage.sh
bash ops/ci/contract-drift.sh

# Produce an explicit full baseline and independently replay it in ratchet mode.
jankurai audit . --full --mode advisory --policy agent/audit-policy.toml --json target/jankurai/accepted-baseline.json --md target/jankurai/accepted-baseline.md --repair-queue-jsonl target/jankurai/repair-queue.jsonl --no-score-history
jankurai audit . --mode ratchet --baseline target/jankurai/accepted-baseline.json --json target/jankurai/repo-score.json --md target/jankurai/repo-score.md --repair-queue-jsonl target/jankurai/repair-queue.jsonl --no-score-history --full

jq -e '.decision.hard_findings == 0 and (.caps_applied | length) == 0' \
  target/jankurai/repo-score.json >/dev/null
while IFS= read -r record || [[ -n "$record" ]]; do
  [[ -n "$record" ]]
  jq -e '
    type == "object" and
    ((keys - ["lane", "owner", "path", "priority", "rule_id", "task", "tlr", "why"]) | length) == 0 and
    (.path | type == "string" and length > 0) and
    (.priority | type == "string" and length > 0) and
    (.task | type == "string" and length > 0) and
    (.why | type == "string" and length > 0)
  ' <<<"$record" >/dev/null
done <target/jankurai/repair-queue.jsonl

printf '%s\n' \
  'language bad-behavior findings are produced by the exact governed audit above' \
  >target/jankurai/language-bad-behavior.log
printf 'tool adoption ok: proof, security, coverage, contract, witness, duplication, and ratchet evidence\n'
