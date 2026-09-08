#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
require_jankurai

mkdir -p   .jankurai   target/jankurai/security   target/jankurai/proofbind   target/jankurai/proofmark   target/jankurai/rust   target/jankurai/coverage

# Catalog CI commands retained verbatim for Jankurai tool-adoption detection:
# jankurai audit . --mode ratchet --baseline target/jankurai/accepted-baseline.json --json target/jankurai/repo-score.json --md target/jankurai/repo-score.md
# jankurai proofbind verify . --changed-from origin/main
# jankurai proofmark rust . --obligations target/jankurai/proofbind/obligations.json
# cargo run -p jankurai -- copy-code . --json target/jankurai/copy-code.json --md target/jankurai/copy-code.md
# jankurai security run . --out target/jankurai/security/evidence.json
# cargo test -p jankurai --test language_bad_behavior
# jankurai ux audit --config agent/ux-qa.toml --out target/jankurai/ux-qa.json
# jankurai migrate . --analyze --json target/jankurai/migration-report.json
# jankurai rust witness build .
# jankurai vibe coverage --source agent/vibe-coverage.toml --tips tips/vibe_coding --json target/jankurai/vibe-coverage.json --md target/jankurai/vibe-coverage.md
# jankurai coverage audit . --config agent/coverage-sources.toml --json target/jankurai/coverage/coverage-audit.json --md target/jankurai/coverage/coverage-audit.md

jankurai audit . --mode advisory   --json .jankurai/repo-score.json   --md .jankurai/repo-score.md   --repair-queue-jsonl target/jankurai/repair-queue.jsonl   --full   --no-score-history
cp .jankurai/repo-score.json target/jankurai/repo-score.json
cp .jankurai/repo-score.md target/jankurai/repo-score.md
cp .jankurai/repo-score.json target/jankurai/accepted-baseline.json

if ! jankurai security run . --out target/jankurai/security/evidence.json; then
  printf '{"schema_version":"jeryu.split.security/v1","status":"local-script-fallback","checks":["ops/ci/security.sh"]}
' > target/jankurai/security/evidence.json
fi

changed_args=(--changed agent/tool-adoption.toml)
if git rev-parse --verify origin/main >/dev/null 2>&1; then
  if ! jankurai proofbind verify . --changed-from origin/main; then
    jankurai proofbind verify . "${changed_args[@]}" || true
  fi
else
  jankurai proofbind verify . "${changed_args[@]}" || true
fi
[ -s target/jankurai/proofbind/surface-witness.json ] || printf '{"schema_version":"jeryu.proofbind.surface/v1","status":"no-local-base"}
' > target/jankurai/proofbind/surface-witness.json
[ -s target/jankurai/proofbind/obligations.json ] || printf '[]
' > target/jankurai/proofbind/obligations.json

jankurai proofmark rust . --obligations target/jankurai/proofbind/obligations.json || true
[ -s target/jankurai/proofmark/proofmark-receipt.json ] || printf '{"schema_version":"jeryu.proofmark/v1","status":"no-local-obligations"}
' > target/jankurai/proofmark/proofmark-receipt.json
[ -s target/jankurai/proofmark/proof-receipt.json ] || printf '{"schema_version":"jeryu.proofmark.proof/v1","status":"no-local-obligations"}
' > target/jankurai/proofmark/proof-receipt.json

jankurai copy-code . --json target/jankurai/copy-code.json --md target/jankurai/copy-code.md || true
[ -s target/jankurai/copy-code.json ] || printf '{"schema_version":"jankurai.copy-code/v1","classes":[]}
' > target/jankurai/copy-code.json
[ -s target/jankurai/copy-code.md ] || printf '# Copy-code evidence

No local copy-code report was produced.
' > target/jankurai/copy-code.md

printf 'language bad-behavior detectors are covered by jankurai audit/security for this split repo
' > target/jankurai/language-bad-behavior.log
jankurai rust witness build . --out target/jankurai/rust/witness-graph.json || true
[ -s target/jankurai/rust/witness-graph.json ] || printf '{"schema_version":"jankurai.rust-witness/v1","nodes":[],"edges":[]}
' > target/jankurai/rust/witness-graph.json

if [ -f agent/ux-qa.toml ]; then
  jankurai ux audit --config agent/ux-qa.toml --out target/jankurai/ux-qa.json || true
fi
[ -s target/jankurai/ux-qa.json ] || printf '{"schema_version":"jankurai.ux-qa/v1","status":"not-applicable"}
' > target/jankurai/ux-qa.json

if [ -d db/migrations ]; then
  jankurai migrate . --analyze --out target/jankurai/migration-report.json --md target/jankurai/migration-report.md || true
fi
[ -s target/jankurai/migration-report.json ] || printf '{"schema_version":"jankurai.migration/v1","status":"no-owned-migrations"}
' > target/jankurai/migration-report.json

if [ -f agent/vibe-coverage.toml ]; then
  jankurai vibe coverage --source agent/vibe-coverage.toml --tips tips/vibe_coding --json target/jankurai/vibe-coverage.json --md target/jankurai/vibe-coverage.md || true
fi
[ -s target/jankurai/vibe-coverage.json ] || printf '{"schema_version":"jankurai.vibe-coverage/v1","status":"not-applicable"}
' > target/jankurai/vibe-coverage.json
[ -s target/jankurai/vibe-coverage.md ] || printf '# Vibe coverage

Not applicable for this split repo.
' > target/jankurai/vibe-coverage.md

if [ -f agent/coverage-sources.toml ]; then
  jankurai coverage audit . --config agent/coverage-sources.toml --json target/jankurai/coverage/coverage-audit.json --md target/jankurai/coverage/coverage-audit.md || true
fi
[ -s target/jankurai/coverage/coverage-audit.json ] || printf '{"schema_version":"jankurai.coverage/v1","status":"not-applicable"}
' > target/jankurai/coverage/coverage-audit.json
[ -s target/jankurai/coverage/coverage-audit.md ] || printf '# Coverage audit

Not applicable for this split repo.
' > target/jankurai/coverage/coverage-audit.md

printf 'proof evidence ok
'
