#!/usr/bin/env bash
set -euo pipefail
required=(
  AGENTS.md
  agent/JANKURAI_STANDARD.md
  agent/owner-map.json
  agent/test-map.json
  agent/proof-lanes.toml
  agent/generated-zones.toml
  agent/standard-version.toml
  agent/baselines/main.repo-score.json
  ops/ci/common.sh
  ops/ci/jankurai.sh
  ops/ci/fast.sh
  ops/ci/full.sh
  ops/ci/audit.sh
  ops/ci/security.sh
  ops/ci/release.sh
  scripts/ci-local.sh
  scripts/ci-doctor.sh
  Justfile
  rust-toolchain.toml
  config/jeryu-cache-policy.toml
  config/trust-tiers.toml
  policies/cache-laws.toml
)
for path in "${required[@]}"; do
  test -f "$path" || { echo "missing required file: $path" >&2; exit 1; }
done
for script in ops/ci/*.sh scripts/*.sh tests/*.sh; do
  [[ "$(basename "$script")" == ._* ]] && continue
  bash -n "$script"
done
json_fixtures=(
  agent/owner-map.json
  agent/test-map.json
  agent/baselines/main.repo-score.json
  examples/cache-key-material.json
  examples/fork-pr-write-request.json
  examples/t1-green-write-request.json
)
for raw in "${json_fixtures[@]}"; do
  [[ "$(basename "$raw")" == ._* ]] && continue
  jq -e . "$raw" >/dev/null || { echo "invalid json: $raw" >&2; exit 1; }
done
printf '%s\n' 'json fixtures ok'

# Rust governance gates (replace the legacy scripts/*.py validators). Prefer the
# prebuilt release binaries; fall back to `cargo run` when they are absent.
jeryu_gate() {
  local crate="$1"; shift
  local bin="target/release/${crate}"
  if [ -x "${bin}" ]; then
    "${bin}" "$@"
  else
    cargo run -q --release -p "${crate}" -- "$@"
  fi
}

jeryu_gate jeryu-mapcheck docs
jeryu_gate jeryu-mapcheck generated-zones
./scripts/check-owner-test-map.sh
./scripts/check-agent-maps.sh
jeryu_gate jeryu-mapcheck fixtures
jeryu_gate jeryu-repogate security-scan
printf '%s\n' 'ci-doctor passed'
