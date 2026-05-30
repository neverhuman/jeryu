#!/usr/bin/env bash
set -euo pipefail

required_roots=(agent bins config configs crates docs examples fixtures ops policies scripts tests)

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

jq -e '.owners | to_entries | all(.key != "" and (.value | tostring) != "")' agent/owner-map.json >/dev/null
jq -e '.tests | to_entries | all(.key != "" and .value.command and .value.lane)' agent/test-map.json >/dev/null

echo "agent maps cover repository paths"
