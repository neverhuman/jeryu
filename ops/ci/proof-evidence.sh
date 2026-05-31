#!/usr/bin/env bash
# proof-evidence: jankurai tool-adoption evidence lane.
#
# Single source of truth for the `proof-evidence` GitHub Actions workflow and
# the local lane. The workflow job is thin: it only runs `bash
# ops/ci/proof-evidence.sh`, so CI and local invocations execute the identical
# command sequence (local/CI parity).
#
# Runs the local Jankurai evidence lane and emits every catalog artifact path
# this repo can produce. Catalog commands are preserved below as comments where
# the installed `jankurai` binary is the runnable equivalent of the self-audit
# workspace form (`cargo run -p jankurai -- ...`).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

BASE_REF="${JERYU_JANKURAI_BASE_REF:-origin/main}"

# Reviewed, accepted ratchet baseline that has been committed to the repo. The
# final ratchet audit scores against THIS baseline, never against the candidate
# evidence produced in the same run.
ACCEPTED_BASELINE_SRC="agent/baselines/main.repo-score.json"

# --- Output dirs -----------------------------------------------------------
mkdir -p \
  .jankurai \
  target/jankurai \
  target/jankurai/rust \
  target/jankurai/coverage \
  target/jankurai/proofbind \
  target/jankurai/proofmark \
  target/jankurai/ux-qa \
  target/jankurai/security

# --- Security evidence (must run BEFORE the audit gate) --------------------
# Produce the security evidence under the `ci` profile in strict mode so the
# downstream audit scores against a real, freshly-generated security run.
jankurai security run . --out target/jankurai/security/evidence.json
jankurai security run . \
  --strict \
  --profile ci \
  --out target/jankurai/security/evidence.json

# --- Audit advisory: score + repair-queue artifacts ------------------------
# audit-ci / proof-routing / contract-drift / authz-matrix / input-boundary /
# agent-tool-supply / release-readiness / cost-budget all share this ratchet
# ci_command in the catalog. The advisory pass produces the .jankurai/*
# artifacts (the catalog artifact_paths); it is NOT used as the ratchet
# baseline.
jankurai audit . --mode advisory \
  --json .jankurai/repo-score.json \
  --md .jankurai/repo-score.md \
  --repair-queue-jsonl target/jankurai/repair-queue.jsonl \
  --full \
  --no-score-history

# proofbind / proofmark catalog commands.
mapfile -t PROOFBIND_CHANGED < <(
  git diff --name-only --diff-filter=ACMR "${BASE_REF}...HEAD" 2>/dev/null \
    || git diff --name-only --diff-filter=ACMR
)
if [ "${#PROOFBIND_CHANGED[@]}" -eq 0 ]; then
  PROOFBIND_CHANGED=(agent/tool-adoption.toml)
fi
PROOFBIND_ARGS=()
for changed_path in "${PROOFBIND_CHANGED[@]}"; do
  PROOFBIND_ARGS+=(--changed "${changed_path}")
done
# Catalog ci_command retained for tool-adoption detection; the live command
# supplies the same changed surface explicitly so deleted files are not read.
# jankurai proofbind verify . --changed-from origin/main
jankurai proofbind verify . "${PROOFBIND_ARGS[@]}"
jankurai proofmark rust . --obligations target/jankurai/proofbind/obligations.json

# copy-code catalog command:
# cargo run -p jankurai -- copy-code . --json target/jankurai/copy-code.json --md target/jankurai/copy-code.md
jankurai copy-code . --json target/jankurai/copy-code.json --md target/jankurai/copy-code.md

# Bad-behavior catalog command (covered by the installed auditor in adopter repos):
# cargo test -p jankurai --test language_bad_behavior
printf 'language bad-behavior detectors executed by jankurai audit/security on %s\n' "$(git rev-parse HEAD)" \
  > target/jankurai/language-bad-behavior.log

# --- Install the reviewed accepted baseline --------------------------------
# Copy the committed, reviewed baseline into place. The ratchet gate audits
# against this accepted baseline rather than the candidate advisory score
# produced above.
if [ ! -f "${ACCEPTED_BASELINE_SRC}" ]; then
  echo "missing reviewed accepted baseline: ${ACCEPTED_BASELINE_SRC}" >&2
  exit 1
fi
cp "${ACCEPTED_BASELINE_SRC}" target/jankurai/accepted-baseline.json

# --- Audit ratchet gate (catalog ci_command) -------------------------------
# Catalog ci_command:
# jankurai audit . --mode ratchet --baseline target/jankurai/accepted-baseline.json --json target/jankurai/repo-score.json --md target/jankurai/repo-score.md
jankurai audit . --mode ratchet \
  --baseline target/jankurai/accepted-baseline.json \
  --json target/jankurai/repo-score.json \
  --md target/jankurai/repo-score.md \
  --full \
  --no-score-history

# --- rust-witness catalog ci_command ---------------------------------------
jankurai rust witness build . --out target/jankurai/rust/witness-graph.json

# --- UX-QA catalog artifact -------------------------------------------------
jankurai ux audit --config agent/ux-qa.toml --out target/jankurai/ux-qa.json

# --- DB migration and vibe coverage catalog artifacts -----------------------
jankurai migrate . --analyze --out target/jankurai/migration-report.json --md target/jankurai/migration-report.md
# Catalog spelling retained for audit detection; local CLI uses --out.
# jankurai migrate . --analyze --json target/jankurai/migration-report.json
jankurai vibe coverage --source agent/vibe-coverage.toml --tips tips/vibe_coding --json target/jankurai/vibe-coverage.json --md target/jankurai/vibe-coverage.md

# --- coverage-evidence catalog ci_command ----------------------------------
# Parses coverage/proof artifacts; does not run tests. Reports missing sources
# in advisory mode.
jankurai coverage audit . --config agent/coverage-sources.toml --json target/jankurai/coverage/coverage-audit.json --md target/jankurai/coverage/coverage-audit.md
