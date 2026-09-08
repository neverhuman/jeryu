set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

fast:
  bash scripts/ci.sh source

check:
  bash scripts/ci.sh source
  cargo fmt --all -- --check

rust:
  bash scripts/ci.sh rust

web:
  bash scripts/ci.sh web

contracts:
  bash scripts/contracts.sh --check

runtime:
  bash scripts/ci.sh runtime

public:
  bash scripts/ci.sh public

sandbox:
  bash scripts/ci.sh sandbox

legacy:
  bash scripts/ci.sh legacy

ci:
  bash scripts/ci.sh all

score:
  ./ops/ci/score.sh # jankurai audit repo-score

security:
  bash scripts/ci.sh security

artifact-support:
  ./ops/ci/artifact_support.sh

profile:
  printf '%s\n' "rust-workspace"
