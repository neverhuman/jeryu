#!/usr/bin/env bash
# Shared CI environment defaults. Source this file; do not execute it.

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  echo "ci-env.sh must be sourced" >&2
  exit 2
fi

export JERYU_CI_JOBS="${JERYU_CI_JOBS:-40}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-${JERYU_CI_JOBS}}"
export CARGO_NET_RETRY="${CARGO_NET_RETRY:-10}"
# The workcell-export CI-seeding unit tests assert a deterministic check-run
# conclusion; they exercise the seeding/recording flow, not real in-process job
# execution (which is non-deterministic under cargo-test). JERYU_CI_MOCK makes
# ci_bridge::run_job return a synthetic conclusion (crates/jeryu-api/src/ci_bridge.rs).
# Production (jeryu-api.service) runs without it and executes jobs for real.
export JERYU_CI_MOCK="${JERYU_CI_MOCK:-1}"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  export JERYU_CI_PROFILE="${JERYU_CI_PROFILE:-github}"
else
  export JERYU_CI_PROFILE="${JERYU_CI_PROFILE:-local}"
fi

# Jeryu-native CI defaults to dockerless Rust execution. Container isolation is
# opt-in for jobs that actually require it.
export JERYU_RUNNER_EXECUTOR="${JERYU_RUNNER_EXECUTOR:-native}"
if [ "${JERYU_CI_PROFILE}" = "local" ]; then
  export JERYU_RUNNER_CLASS="${JERYU_RUNNER_CLASS:-native-rust-hot}"
else
  export JERYU_RUNNER_CLASS="${JERYU_RUNNER_CLASS:-native-rust-clean}"
fi
export JERYU_CI_DOCKER="${JERYU_CI_DOCKER:-0}"

# Rust cache acceleration is opportunistic and local-first. GitHub falls back to
# ordinary Cargo behavior when sccache is unavailable, but the command path stays
# identical.
if [ "${JERYU_CI_USE_SCCACHE:-1}" != "0" ] && command -v sccache >/dev/null 2>&1; then
  export RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}"
  export SCCACHE_DIR="${SCCACHE_DIR:-${HOME}/.cache/jeryu/sccache}"
  export SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-20G}"
fi

jeryu_ci_profile_summary() {
  echo "ci profile: ${JERYU_CI_PROFILE}"
  echo "workers: ${JERYU_CI_JOBS}"
  echo "cargo build jobs: ${CARGO_BUILD_JOBS}"
  echo "runner executor: ${JERYU_RUNNER_EXECUTOR}"
  echo "runner class: ${JERYU_RUNNER_CLASS}"
  echo "docker required: ${JERYU_CI_DOCKER}"
  echo "rustc wrapper: ${RUSTC_WRAPPER:-none}"
}
