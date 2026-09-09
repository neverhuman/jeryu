#!/usr/bin/env bash
set -euo pipefail

bash ops/ci/fast.sh
bash ops/ci/check.sh
bash ops/ci/property_tests.sh
bash ops/ci/migration_drift.sh
bash ops/ci/contract_drift.sh
bash ops/ci/score.sh
bash ops/ci/security.sh
bash ops/ci/release.sh
bash ops/ci/artifact_support.sh
