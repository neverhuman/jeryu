set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

jobs := env_var_or_default("JERYU_CI_JOBS", "40")

fast:
  ./ops/ci/fast.sh

check:
  ./ops/ci/check.sh

test:
  cargo test --workspace --all-targets --jobs "{{jobs}}"

property-tests:
  ./ops/ci/property_tests.sh

migration-drift:
  ./ops/ci/migration_drift.sh

contract-drift:
  ./ops/ci/contract_drift.sh

score:
  ./ops/ci/score.sh

security:
  ./tools/security-lane.sh

release-readiness:
  ./ops/ci/release.sh

artifact-support:
  ./ops/ci/artifact_support.sh

doctor:
  ./scripts/ci-doctor.sh

ci-local:
  ./scripts/ci-local.sh

profile:
  printf '%s\n' "rust-workspace"
