#!/usr/bin/env bash
set -euo pipefail

required_roots=(.github agent bins config configs crates docs examples fixtures ops policies scripts tests)
required_exact_paths=(.github/ agent/ci-lanes.toml crates/jeryu-repogate/)
parity_sensitive_paths=(.github/ agent/ci-lanes.toml crates/jeryu-repogate/ ops/ scripts/)

for map in agent/owner-map.json agent/test-map.json; do
  jq -e . "$map" >/dev/null
done

for root in "${required_roots[@]}"; do
  jq -e --arg root "$root" '
    .owners
    | keys
    | any((rtrimstr("/") | split("/")[0]) == $root)
  ' agent/owner-map.json >/dev/null || {
    echo "missing owner root: $root" >&2
    exit 1
  }
  jq -e --arg root "$root" '
    .tests
    | keys
    | any((rtrimstr("/") | split("/")[0]) == $root)
  ' agent/test-map.json >/dev/null || {
    echo "missing test root: $root" >&2
    exit 1
  }
done

for path in "${required_exact_paths[@]}"; do
  jq -e --arg path "$path" '.owners | has($path)' agent/owner-map.json >/dev/null || {
    echo "missing exact owner path: $path" >&2
    exit 1
  }
  jq -e --arg path "$path" '.tests | has($path)' agent/test-map.json >/dev/null || {
    echo "missing exact test path: $path" >&2
    exit 1
  }
done

for path in "${parity_sensitive_paths[@]}"; do
  jq -e --arg path "$path" '
    .tests[$path].command | contains("jeryu-repogate -- ci-lanes-check")
  ' agent/test-map.json >/dev/null || {
    echo "missing ci-lanes-check in test-map command for $path" >&2
    exit 1
  }
done

jq -e '.owners | to_entries | all(.key != "" and (.value | tostring) != "")' agent/owner-map.json >/dev/null
jq -e '.tests | to_entries | all(.key != "" and .value.command and .value.lane)' agent/test-map.json >/dev/null

echo "agent maps cover repository paths"
