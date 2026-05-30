#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

mkdir -p .jankurai target/jankurai
jankurai proof . \
  --changed-from "${JERYU_JANKURAI_BASE_REF:-origin/main}" \
  --out target/jankurai/proof-plan.json \
  --md target/jankurai/proof-plan.md
jankurai proofbind map . \
  --changed-from "${JERYU_JANKURAI_BASE_REF:-origin/main}" \
  --mode advisory \
  --out target/jankurai/proofbind/surface-witness.json \
  --obligations-out target/jankurai/proofbind/obligations.json \
  --md target/jankurai/proofbind/proofbind.md
jankurai proofbind verify . \
  --changed-from "${JERYU_JANKURAI_BASE_REF:-origin/main}" \
  --mode advisory \
  --out target/jankurai/proofbind/surface-witness.json \
  --obligations-out target/jankurai/proofbind/obligations.json \
  --md target/jankurai/proofbind/proofbind.md
jankurai proofmark rust . \
  --changed-from "${JERYU_JANKURAI_BASE_REF:-origin/main}" \
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
    --base-ref "${JERYU_JANKURAI_BASE_REF:-origin/main}" \
    --out-dir target/jankurai/diff
fi
