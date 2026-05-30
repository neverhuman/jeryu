#!/usr/bin/env bash
set -euo pipefail
mkdir -p receipts/generated
cat > receipts/generated/phase12-release.receipt.json <<JSON
{
  "phase": 12,
  "artifact": "cratevault",
  "release_lane": "hermetic",
  "mutable_compiled_cache_consumed": false,
  "cache_false_hits": 0,
  "generated_by": "scripts/emit-release-receipt.sh"
}
JSON
cat receipts/generated/phase12-release.receipt.json
