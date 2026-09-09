#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
mkdir -p target/artifact-support
cat > target/artifact-support/jeryu-web.json <<'JSON'
{"schema_version":"jeryu.split.artifact-support/v1","repo":"jeryu-web","status":"bootstrap"}
JSON
printf 'artifact support bootstrap ok\n'
