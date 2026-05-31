#!/usr/bin/env bash
# proof-evidence: jankurai tool-adoption evidence lane.
#
# Single source of truth for the `proof-evidence` GitHub Actions workflow and
# the local lane. The workflow job is thin: it only runs `bash
# ops/ci/proof-evidence.sh`, so CI and local invocations execute the identical
# command sequence (local/CI parity).
#
# Runs the catalog `ci_command` for every jankurai tool that genuinely executes
# in this repo and produces each tool's `artifact_paths`. Tools whose catalog
# ci_command does not run in this repo (cargo run -p jankurai / cargo test -p
# jankurai, the ux-qa node CLI, the security-lane.sh harness as a standalone,
# the vibe_coding tips corpus, db migrations) are intentionally omitted rather
# than faked.
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
  target/jankurai/security

# --- Security evidence (must run BEFORE the audit gate) --------------------
# Produce the security evidence under the `ci` profile in strict mode so the
# downstream audit scores against a real, freshly-generated security run.
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
  --repair-queue-jsonl target/jankurai/repair-queue.jsonl

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
jankurai audit . --mode ratchet \
  --baseline target/jankurai/accepted-baseline.json \
  --json target/jankurai/repo-score.json \
  --md target/jankurai/repo-score.md

# --- rust-witness catalog ci_command ---------------------------------------
jankurai rust witness build . --out target/jankurai/rust/witness-graph.json

# --- coverage-evidence catalog ci_command ----------------------------------
# Parses coverage/proof artifacts; does not run tests. Reports missing sources
# in advisory mode.
jankurai coverage audit . \
  --config agent/coverage-sources.toml \
  --json target/jankurai/coverage/coverage-audit.json \
  --md target/jankurai/coverage/coverage-audit.md
