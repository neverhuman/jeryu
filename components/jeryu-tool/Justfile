set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

jobs := env_var_or_default("JERYU_TOOL_CI_JOBS", "4")

# One-command setup/validation: run the full local gate.
default:
  ./ops/ci/required.sh

fast:
  ./ops/ci/fast.sh # jankurai pin drift check

# Bounded, locked, package-only feedback using the governed shared compiler cache.
fast-proof:
  RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}" CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/fast-proof/cargo}" cargo check -p jeryu-tool-control --locked --offline --jobs {{jobs}}

fast-test:
  RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}" CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/fast-proof/cargo}" cargo test -p jeryu-tool-control --locked --offline --all-targets --jobs {{jobs}}

fast-coverage:
  ./ops/ci/coverage.sh

check:
  ./ops/ci/check.sh

required:
  ./ops/ci/required.sh

score:
  ./ops/ci/score.sh # jankurai audit repo-score

security:
  ./tools/security-lane.sh # gitleaks actionlint dependency policy and source SBOM

tool-adoption:
  ./ops/ci/tool-adoption.sh

proof-routing:
  ./ops/ci/proof-routing.sh

contract-drift:
  ./ops/ci/contract-drift.sh

repair-receipt-contract:
  ./ops/ci/repair-receipt-test.sh

repair-proof:
  ./ops/ci/contract-drift.sh
  ./ops/ci/repair-receipt-test.sh

artifact-support:
  ./ops/ci/artifact_support.sh

profile:
  printf '%s\n' "public-portal"
