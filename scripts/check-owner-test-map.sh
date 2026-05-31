#!/usr/bin/env bash
set -euo pipefail

required_patterns=(
  ".github/"
  "agent/"
  "agent/ci-lanes.toml"
  "bins/"
  "config/"
  "configs/"
  "crates/"
  "crates/jeryu-repogate/"
  "docs/"
  "examples/"
  "fixtures/"
  "ops/"
  "policies/"
  "scripts/"
  "tests/"
)

for pattern in "${required_patterns[@]}"; do
  jq -e --arg pattern "$pattern" '.owners | has($pattern)' agent/owner-map.json >/dev/null || {
    echo "missing owner path: ${pattern}" >&2
    exit 1
  }
  jq -e --arg pattern "$pattern" '.tests | has($pattern)' agent/test-map.json >/dev/null || {
    echo "missing test path: ${pattern}" >&2
    exit 1
  }
done

jq -e '.owners | type == "object" and length > 0' agent/owner-map.json >/dev/null
jq -e '.tests | type == "object" and length > 0' agent/test-map.json >/dev/null

echo "owner/test map ok"
