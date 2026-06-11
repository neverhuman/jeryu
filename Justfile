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

profile:
  printf '%s\n' "public-portal"
