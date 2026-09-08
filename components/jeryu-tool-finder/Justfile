set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Full product gate (no `fast`/pin lane — the jankurai pin is owned by jeryu-tool).
default:
  ./ops/ci/check.sh
  ./ops/ci/score.sh
  ./ops/ci/security.sh
  ./ops/ci/contract-drift.sh
  ./ops/ci/artifact-support.sh
  ./tests/artifact_support_test.sh

check:
  ./ops/ci/check.sh   # cargo fmt/clippy/test + shell syntax

score:
  ./ops/ci/score.sh   # jankurai audit repo-score

security:
  ./ops/ci/security.sh # gitleaks actionlint env-file

contract-drift:
  ./ops/ci/contract-drift.sh # tracked CLI help contract

artifact-support:
  ./ops/ci/artifact-support.sh # exact-head CLI + score/security receipt
  ./tests/artifact_support_test.sh

# Deterministic dependency policy subset; advisories are enforced by security.
security-deps:
  cargo deny check bans licenses sources --disable-fetch

build:
  cargo build --release

# Discovery surface (all-Rust; the engine is linked as a library).
scan *ARGS:
  cargo run --release -- scan {{ARGS}}

dossier *ARGS:
  cargo run --release -- dossier {{ARGS}}

propose CLUSTER *ARGS:
  cargo run --release -- propose {{CLUSTER}} {{ARGS}}

summary *ARGS:
  cargo run --release -- summary {{ARGS}}

profile:
  printf '%s\n' "public-portal"
