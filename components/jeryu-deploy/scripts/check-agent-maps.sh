#!/usr/bin/env bash
set -euo pipefail

required_exact_paths=(.github/ agent/ci-lanes.toml crates/jeryu-split-tool/)
parity_sensitive_paths=(.github/ agent/ci-lanes.toml crates/jeryu-split-tool/ ops/ scripts/ tools/)

for map in agent/owner-map.json agent/test-map.json; do
  jq -e . "$map" >/dev/null
done

while IFS= read -r -d '' path; do
  jq -e --arg path "$path" '
    .owners
    | to_entries
    | any(.key as $key
        | ($path == $key)
          or (($key | endswith("/")) and ($path | startswith($key))))
  ' agent/owner-map.json >/dev/null || {
    echo "missing owner coverage for tracked path: $path" >&2
    exit 1
  }
  jq -e --arg path "$path" '
    .tests
    | to_entries
    | any(.key as $key
        | ($path == $key)
          or (($key | endswith("/")) and ($path | startswith($key))))
  ' agent/test-map.json >/dev/null || {
    echo "missing test coverage for tracked path: $path" >&2
    exit 1
  }
done < <(git ls-files -z)

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
    .tests[$path].command | contains("ops/ci/workflow-lint.sh")
  ' agent/test-map.json >/dev/null || {
    echo "missing workflow parity gate in test-map command for $path" >&2
    exit 1
  }
done

jq -e '.owners | to_entries | all(.key != "" and (.value | tostring) != "")' agent/owner-map.json >/dev/null
jq -e '.tests | to_entries | all(.key != "" and .value.command and .value.lane)' agent/test-map.json >/dev/null

echo "agent maps cover repository paths"
