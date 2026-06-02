#!/usr/bin/env bash
# ops/ci/shard.sh — run shard i of N of the workspace test set.
#
# Usage:
#   bash ops/ci/shard.sh <i> <N> [-- extra cargo-nextest args...]
#
#   i   shard index, 0-based, in the range [0, N)
#   N   total number of shards, N >= 1
#
# The positional arguments may also be supplied as JERYU_CI_SHARD_INDEX and
# JERYU_CI_SHARD_TOTAL. Hosted CI defaults the shard total to 40 when omitted.
#
# Sharding is delegated to cargo-nextest's native `count` partitioner
# (`--partition count:M/N`, 1-based M). This is the house mechanism that the
# jeryu-rustjet NextestPlanner emits and that the shard-union property test in
# crates/jeryu-rustjet proves to be exhaustive and disjoint: the union of all N
# shards equals the full workspace test set exactly once (no test runs twice,
# none is missed).
#
# Thread count honors JERYU_CI_SHARD_JOBS, defaulting to 2 per shard and still
# clamped by JERYU_CI_JOBS, nproc, and the global 40-worker ceiling.
set -euo pipefail

# lib.sh -> common.sh -> ci-env.sh: pulls in ci_log plus JERYU_CI_* env defaults.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

jeryu_positive_int() {
  case "$1" in
    ''|*[!0-9]*) return 1 ;;
    *) [ "$1" -ge 1 ] ;;
  esac
}

jeryu_nonnegative_int() {
  case "$1" in
    ''|*[!0-9]*) return 1 ;;
    *) return 0 ;;
  esac
}

jeryu_shard_test_threads() {
  local cores ceiling=40 jobs
  jobs="${JERYU_CI_SHARD_JOBS:-2}"
  if ! jeryu_positive_int "${jobs}"; then
    echo "shard.sh: JERYU_CI_SHARD_JOBS must be a positive integer, got '${jobs}'" >&2
    return 2
  fi

  cores="$(nproc 2>/dev/null || echo 1)"
  if ! jeryu_positive_int "${cores}"; then
    cores=1
  fi

  if [ "${cores}" -lt "${jobs}" ]; then
    jobs="${cores}"
  fi
  if [ "${ceiling}" -lt "${jobs}" ]; then
    jobs="${ceiling}"
  fi

  if ! jeryu_positive_int "${JERYU_CI_JOBS:-}"; then
    echo "shard.sh: JERYU_CI_JOBS must be a positive integer, got '${JERYU_CI_JOBS:-}'" >&2
    return 2
  fi
  if [ "${JERYU_CI_JOBS}" -lt "${jobs}" ]; then
    jobs="${JERYU_CI_JOBS}"
  fi

  printf '%s\n' "${jobs}"
}

jeryu_require_native_rust_shard() {
  if [ "${JERYU_CI_DOCKER}" != "0" ]; then
    echo "shard.sh: Rust shards require dockerless native execution (JERYU_CI_DOCKER=${JERYU_CI_DOCKER})" >&2
    return 1
  fi
  if [ "${JERYU_RUNNER_EXECUTOR}" != "native" ]; then
    echo "shard.sh: Rust shards require JERYU_RUNNER_EXECUTOR=native, got '${JERYU_RUNNER_EXECUTOR}'" >&2
    return 1
  fi
  case "${JERYU_RUNNER_CLASS}" in
    native-rust-clean|native-rust-hot) ;;
    *)
      echo "shard.sh: Rust shards require a native Rust runner class, got '${JERYU_RUNNER_CLASS}'" >&2
      return 1
      ;;
  esac
}

usage() {
  cat >&2 <<'EOF'
usage: shard.sh <i> <N> [-- extra cargo-nextest args...]
  i   shard index, 0-based, 0 <= i < N
  N   total shard count, N >= 1
env:
  JERYU_CI_SHARD_INDEX  shard index fallback
  JERYU_CI_SHARD_TOTAL  shard total fallback; hosted CI defaults to 40
  JERYU_CI_SHARD_JOBS   per-shard test threads; defaults to 2
EOF
}

main() {
  local i="${JERYU_CI_SHARD_INDEX:-}"
  local n="${JERYU_CI_SHARD_TOTAL:-}"

  if [ "$#" -gt 0 ] && [ "${1:-}" != "--" ]; then
    i="$1"
    shift
  fi
  if [ "$#" -gt 0 ] && [ "${1:-}" != "--" ]; then
    n="$1"
    shift
  fi
  if [ -z "${n}" ] && [ "${JERYU_CI_PROFILE}" != "local" ]; then
    n=40
  fi

  # Strip an optional leading `--` separating positional args from passthrough.
  if [ "${1:-}" = "--" ]; then
    shift
  fi

  if [ -z "${i}" ] || [ -z "${n}" ]; then
    usage
    exit 2
  fi

  if ! jeryu_nonnegative_int "${i}"; then
    echo "shard.sh: shard index must be a non-negative integer, got '${i}'" >&2
    exit 2
  fi
  if ! jeryu_positive_int "${n}"; then
    echo "shard.sh: shard count N must be >= 1, got '${n}'" >&2
    exit 2
  fi
  if [ "${i}" -ge "${n}" ]; then
    echo "shard.sh: shard index i must satisfy 0 <= i < N (got i=${i}, N=${n})" >&2
    exit 2
  fi

  # cargo-nextest partitions are 1-based: shard 0 -> count:1/N.
  local partition="count:$(( i + 1 ))/${n}"
  local jobs
  jobs="$(jeryu_shard_test_threads)"

  jeryu_require_native_rust_shard
  if ! command -v cargo-nextest >/dev/null 2>&1; then
    echo "shard.sh: cargo-nextest is required for native Rust sharding" >&2
    exit 1
  fi

  ci_log "shard ${i}/${n} -> nextest --partition ${partition} (test-threads=${jobs})"

  cargo nextest run \
    --profile "${JERYU_CI_PROFILE_NEXTEST:-ci}" \
    --workspace \
    --partition "${partition}" \
    --test-threads "${jobs}" \
    "$@"
}

main "$@"
