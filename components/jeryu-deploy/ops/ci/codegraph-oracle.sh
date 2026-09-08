#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

JOBS="${JERYU_CI_JOBS:-40}"

echo "codegraph-oracle: schema v3 storage + MCP/API contract"
cargo test --locked -p jeryu-codegraph --jobs "$JOBS"
cargo test --locked -p jeryu-mcp --test mcp_conformance --jobs "$JOBS"
cargo test --locked -p jeryu-api --features web --jobs "$JOBS" codegraph
