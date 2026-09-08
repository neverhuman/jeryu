set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

jobs := env_var_or_default("JERYU_CI_JOBS", "40")

fast:
  ./ops/ci/fast.sh # cargo check

check:
  ./ops/ci/check.sh

score:
  ./ops/ci/score.sh # jankurai audit repo-score

security:
  JERYU_SECURITY_NETWORK=1 ./tools/security-lane.sh

contract-drift:
  cargo test -p jeryu-runner-protocol --locked --test schema_contract_drift

artifact-support:
  ./ops/ci/artifact_support.sh

profile:
  printf '%s\n' "rust-workspace"
