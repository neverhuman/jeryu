set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

jobs := env_var_or_default("JERYU_CI_JOBS", "40")

fast:
  ./ops/ci/fast.sh # cargo check

check:
  ./ops/ci/check.sh

score:
  ./ops/ci/score.sh # jankurai audit repo-score

security:
  ./ops/ci/security.sh # gitleaks cargo audit npm audit syft

artifact-support:
  ./ops/ci/artifact_support.sh

redline-consumer-test:
  cargo test --locked -p jeryu-obs --test redline_consumer_contract
  bash ops/ci/redline-consumer-authority-test.sh

release-readiness: fast check score security artifact-support redline-consumer-test

profile:
  printf '%s\n' "rust-workspace"
