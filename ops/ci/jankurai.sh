#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

mkdir -p .jankurai target/jankurai
BASE_REF="${JERYU_JANKURAI_BASE_REF:-origin/main}"
mapfile -t JANKURAI_CHANGED < <(
  git diff --name-only --diff-filter=ACMR "${BASE_REF}...HEAD" 2>/dev/null \
    || git diff --name-only --diff-filter=ACMR
)
if [ "${#JANKURAI_CHANGED[@]}" -eq 0 ]; then
  JANKURAI_CHANGED=(agent/tool-adoption.toml)
fi
JANKURAI_CHANGED_ARGS=()
for changed_path in "${JANKURAI_CHANGED[@]}"; do
  JANKURAI_CHANGED_ARGS+=(--changed "${changed_path}")
done

jankurai proof \
  "${JANKURAI_CHANGED_ARGS[@]}" \
  --out target/jankurai/proof-plan.json \
  --md target/jankurai/proof-plan.md \
  .
jankurai proofbind map . \
  "${JANKURAI_CHANGED_ARGS[@]}" \
  --mode advisory \
  --out target/jankurai/proofbind/surface-witness.json \
  --obligations-out target/jankurai/proofbind/obligations.json \
  --md target/jankurai/proofbind/proofbind.md
jankurai proofbind verify . \
  "${JANKURAI_CHANGED_ARGS[@]}" \
  --mode advisory \
  --out target/jankurai/proofbind/surface-witness.json \
  --obligations-out target/jankurai/proofbind/obligations.json \
  --md target/jankurai/proofbind/proofbind.md
jankurai proofmark rust . \
  "${JANKURAI_CHANGED_ARGS[@]}" \
  --mode advisory \
  --obligations target/jankurai/proofbind/obligations.json \
  --out target/jankurai/proofmark/proofmark-receipt.json \
  --proof-receipt target/jankurai/proofmark/proof-receipt.json \
  --md target/jankurai/proofmark/proofmark.md
jankurai copy-code . \
  --json target/jankurai/copy-code.json \
  --md target/jankurai/copy-code.md
jankurai rust map . --out-dir target/jankurai/rust
jankurai rust witness build . --out target/jankurai/rust/witness-graph.json
jankurai rust diagnose . --out target/jankurai/rust/compile-packets.json
jankurai security run . \
  --script ./ops/ci/security.sh \
  --out target/jankurai/security/evidence.json \
  --profile local
if [[ "${JERYU_JANKURAI_FULL:-0}" == "1" ]]; then
  jankurai . \
    --json .jankurai/repo-score.json \
    --md .jankurai/repo-score.md \
    --fail-under "${JERYU_JANKURAI_FAIL_UNDER:-85}"
else
  jankurai diff-audit . \
    --base-ref "${BASE_REF}" \
    --out-dir target/jankurai/diff
fi
