#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
cd "$REPO_ROOT"

bash ops/ci/fast.sh
bash ops/ci/check.sh
bash ops/ci/repair-receipt-test.sh
bash ops/ci/contract-drift.sh
bash ops/ci/artifact_support.sh
bash ops/ci/score.sh
printf 'required ok: jeryu-tool\n'
