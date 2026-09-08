set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

jobs := env_var_or_default("JERYU_CI_JOBS", "40")

fast:
  ./ops/ci/fast.sh # cargo check

check:
  ./ops/ci/check.sh

score:
  ./ops/ci/score.sh # jankurai audit repo-score

security:
  bash tools/security-lane.sh # gitleaks detect · npm audit --audit-level=high · cargo audit · zizmor · syft SBOM · dependency-review

artifact-support:
  ./ops/ci/artifact_support.sh

e2e:
  ./ops/ci/e2e.sh

profile:
  printf '%s\n' "node-frontend"
