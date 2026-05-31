#!/usr/bin/env bash
# ops/ci/shard.sh — run shard i of N of the workspace test set.
#
# Usage:
#   bash ops/ci/shard.sh <i> <N> [-- extra cargo-nextest args...]
#
#   i   shard index, 0-based, in the range [0, N)
#   N   total number of shards, N >= 1
#
# Sharding is delegated to cargo-nextest's native `count` partitioner
# (`--partition count:M/N`, 1-based M). This is the house mechanism that the
# jeryu-rustjet NextestPlanner emits and that the shard-union property test in
# crates/jeryu-rustjet proves to be exhaustive and disjoint: the union of all N
# shards equals the full workspace test set exactly once (no test runs twice,
# none is missed).
#
# Thread count honors JERYU_CI_JOBS, clamped to min(40, nproc) by
# jeryu_shard_clamp_jobs below.
set -euo pipefail

# lib.sh -> common.sh -> ci-env.sh: pulls in ci_log plus JERYU_CI_* env defaults.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

# jeryu_shard_clamp_jobs — echo min(40, nproc), or min(40, nproc, $JERYU_CI_JOBS)
# when JERYU_CI_JOBS is already set in the environment.
#
# NOTE: the canonical home for this clamp is arguably ops/ci/ci-env.sh (where
# JERYU_CI_JOBS defaults to 40) or ops/ci/common.sh, but those are shared
# manifests this lane must not edit. See followups.
jeryu_shard_clamp_jobs() {
  local cores ceiling=40 jobs
  cores="$(nproc 2>/dev/null || echo 1)"
  # Start from the smaller of the ceiling and the core count.
  if [ "${cores}" -lt "${ceiling}" ]; then
    jobs="${cores}"
  else
    jobs="${ceiling}"
  fi
  # If JERYU_CI_JOBS was provided, never exceed it either.
  if [ -n "${JERYU_CI_JOBS:-}" ] && [ "${JERYU_CI_JOBS}" -lt "${jobs}" ]; then
    jobs="${JERYU_CI_JOBS}"
  fi
  printf '%s\n' "${jobs}"
}

usage() {
  cat >&2 <<'EOF'
usage: shard.sh <i> <N> [-- extra cargo-nextest args...]
  i   shard index, 0-based, 0 <= i < N
  N   total shard count, N >= 1
EOF
}

main() {
  if [ "$#" -lt 2 ]; then
    usage
    exit 2
  fi

  local i="$1" n="$2"
  shift 2

  # Strip an optional leading `--` separating positional args from passthrough.
  if [ "${1:-}" = "--" ]; then
    shift
  fi

  case "${i}" in
    ''|*[!0-9]*) echo "shard.sh: shard index must be a non-negative integer, got '${i}'" >&2; exit 2 ;;
  esac
  case "${n}" in
    ''|*[!0-9]*) echo "shard.sh: shard count must be a positive integer, got '${n}'" >&2; exit 2 ;;
  esac
  if [ "${n}" -lt 1 ]; then
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
  jobs="$(jeryu_shard_clamp_jobs)"

  ci_log "shard ${i}/${n} -> nextest --partition ${partition} (test-threads=${jobs})"

  cargo nextest run \
    --profile "${JERYU_CI_PROFILE_NEXTEST:-ci}" \
    --workspace \
    --partition "${partition}" \
    --test-threads "${jobs}" \
    "$@"
}

main "$@"
