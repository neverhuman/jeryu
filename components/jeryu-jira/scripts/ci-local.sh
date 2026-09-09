#!/usr/bin/env bash
set -euo pipefail

./scripts/ci-doctor.sh
./ops/ci/fast.sh
./ops/ci/check.sh
./ops/ci/property_tests.sh
./ops/ci/migration_drift.sh
./ops/ci/contract_drift.sh
./ops/ci/score.sh
./ops/ci/security.sh
./ops/ci/release.sh
./ops/ci/artifact_support.sh
