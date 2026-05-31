set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Parallelism shared by every recipe so local runs match CI.
jobs := env_var_or_default("JERYU_CI_JOBS", "40")

# Deterministic fast lane: format, narrow check, and core/cache tests.
fast:
  ./ops/ci/fast.sh

# Full local CI lane.
full:
  ./ops/ci/full.sh

ci:
  ./scripts/ci-phases.sh

audit:
  ./ops/ci/audit.sh

security:
  ./ops/ci/security-tools.sh
  ./ops/ci/security.sh

release:
  ./ops/ci/release.sh

score:
  ./scripts/ci-doctor.sh

doctor:
  ./scripts/ci-doctor.sh

# --- Narrow proof lanes for agent iteration -------------------------------
# Each lane runs the smallest deterministic command that proves one surface,
# so an agent can re-prove a single change without a full-workspace rebuild.

# Cheapest signal: type-check the whole workspace without running tests.
check:
  cargo check --workspace --all-targets --jobs {{jobs}}

# Run one crate's tests, e.g. `just test jeryu-gitd`.
test crate:
  cargo nextest run -p {{crate}} --jobs {{jobs}}

# Rendered UX proof lanes.
ux-qa-build:
  npm --workspace @jankurai/ux-qa run build

ux-qa-test:
  npm --workspace @jankurai/ux-qa run test

# Cache-law proof lane.
prove-cache:
  cargo nextest run -p jeryu-cache-core -p jeryu-cache-service -p jeryu-cache-adversary --jobs {{jobs}}

# Git server proof lane.
prove-git:
  cargo nextest run -p jeryu-gitd --jobs {{jobs}}

# Release provenance and SBOM proof lane.
prove-provenance:
  cargo nextest run -p jeryu-signrail --jobs {{jobs}}

# Agent map/zone/fixture/doc governance proof lane.
prove-maps:
  ./scripts/check-owner-test-map.sh
  ./scripts/check-agent-maps.sh
  cargo run -q -p jeryu-mapcheck -- generated-zones
  cargo run -q -p jeryu-mapcheck -- db-boundary

phase12-tree:
  find . -type f | sort
