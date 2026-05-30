set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

fast:
  ./ops/ci/fast.sh

full:
  ./ops/ci/full.sh

audit:
  ./ops/ci/audit.sh

security:
  ./ops/ci/security.sh

release:
  ./ops/ci/release.sh

score:
  ./scripts/ci-doctor.sh

phase12-tree:
  find . -type f | sort
