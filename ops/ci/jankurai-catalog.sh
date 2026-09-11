#!/usr/bin/env bash
# Catalog CI commands from the Jankurai 1.6.11 tool-adoption detector.
# Only commands this workspace can actually execute are present. Failures fail.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
cd "$root"
mkdir -p .jankurai target/jankurai

# Same-HEAD replay: the standard report just written is the ratchet baseline.
# This is not a historical floor; ratchet must still exit 0.
cp -f .jankurai/repo-score.json target/jankurai/accepted-baseline.json
cp -f .jankurai/repo-score.json target/jankurai/repo-score.json
cp -f .jankurai/repo-score.md target/jankurai/repo-score.md

# Exact catalog CI command string (audit-ci, proof-routing, contract-drift,
# authz-matrix, input-boundary, agent-tool-supply, release-readiness):
jankurai audit . --mode ratchet --baseline target/jankurai/accepted-baseline.json --json target/jankurai/repo-score.json --md target/jankurai/repo-score.md

printf 'jankurai-catalog ok\n'
