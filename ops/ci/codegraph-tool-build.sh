#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"
source "${ROOT}/ops/ci/common.sh"

mkdir -p target/jankurai
DB="target/jankurai/codegraph-tool-build.sqlite"
rm -f "${DB}"

cargo test -p jeryu-codegraph --jobs "${JERYU_CI_JOBS}" tool_build
cargo test -p jeryu-mcp --test mcp_conformance --jobs "${JERYU_CI_JOBS}"
cargo test -p jeryu-api --features web --jobs "${JERYU_CI_JOBS}" tool_build

cargo run -q -p jeryu-codegraph -- tool-build scan \
  --root . \
  --db "${DB}" \
  --repo-id local/jeryu \
  --commit tool-build-ci \
  --top 10 \
  --json > target/jankurai/codegraph-tool-build-scan.json

cargo run -q -p jeryu-codegraph -- tool-build clusters \
  --db "${DB}" \
  --repo-id local/jeryu \
  --top 10 \
  --json > target/jankurai/codegraph-tool-build-clusters.json

echo "codegraph tool-build lane ok: target/jankurai/codegraph-tool-build-{scan,clusters}.json"
