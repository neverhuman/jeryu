#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"
source "${ROOT}/ops/ci/common.sh"

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

jeryu_jankurai proof \
  "${JANKURAI_CHANGED_ARGS[@]}" \
  --out target/jankurai/proof-plan.json \
  --md target/jankurai/proof-plan.md \
  .
jeryu_jankurai proofbind map . \
  "${JANKURAI_CHANGED_ARGS[@]}" \
  --mode advisory \
  --out target/jankurai/proofbind/surface-witness.json \
  --obligations-out target/jankurai/proofbind/obligations.json \
  --md target/jankurai/proofbind/proofbind.md
jeryu_jankurai proofbind verify . \
  "${JANKURAI_CHANGED_ARGS[@]}" \
  --mode advisory \
  --out target/jankurai/proofbind/surface-witness.json \
  --obligations-out target/jankurai/proofbind/obligations.json \
  --md target/jankurai/proofbind/proofbind.md
jeryu_jankurai proofmark rust . \
  "${JANKURAI_CHANGED_ARGS[@]}" \
  --mode advisory \
  --obligations target/jankurai/proofbind/obligations.json \
  --out target/jankurai/proofmark/proofmark-receipt.json \
  --proof-receipt target/jankurai/proofmark/proof-receipt.json \
  --md target/jankurai/proofmark/proofmark.md
jeryu_jankurai copy-code . \
  --json target/jankurai/copy-code.json \
  --md target/jankurai/copy-code.md
jeryu_jankurai rust map . --out-dir target/jankurai/rust
jeryu_jankurai rust witness build . --out target/jankurai/rust/witness-graph.json
jeryu_jankurai rust diagnose . --out target/jankurai/rust/compile-packets.json
bash ops/ci/security-tools.sh
jeryu_jankurai security run . \
  --script ./ops/ci/security.sh \
  --out target/jankurai/security/evidence.json \
  --profile local
raw_policy="target/jankurai/raw-audit-policy.toml"
jeryu_raw_policy "${raw_policy}"
jeryu_jankurai audit . \
  --policy "${raw_policy}" \
  --json target/jankurai/raw-repo-score.json \
  --md target/jankurai/raw-repo-score.md \
  --no-score-history

# Catalog ci_command:
# jankurai . --json .jankurai/repo-score.json --md .jankurai/repo-score.md
# The full-repo audit is the FULL-mode quality bar (it carries pre-existing
# main-branch caps like release.yml's ci-bad-behavior). PR/CI runs gate on the
# scoped diff-audit so a PR is judged on the files it actually changed.
if [[ "${JERYU_JANKURAI_FULL:-0}" == "1" ]]; then
  jeryu_jankurai . \
    --json .jankurai/repo-score.json \
    --md .jankurai/repo-score.md \
    --fail-under "${JERYU_JANKURAI_FAIL_UNDER:-85}"
else
  jeryu_jankurai diff-audit . \
    --base-ref "${BASE_REF}" \
    --out-dir target/jankurai/diff
fi
