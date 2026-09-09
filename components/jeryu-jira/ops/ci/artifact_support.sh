#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
mkdir -p target/artifact-support
cat > target/artifact-support/jeryu-jira.json <<'JSON'
{"schema_version":"jeryu.split.artifact-support/v1","repo":"jeryu-jira","status":"ready","artifacts":["target/jankurai/repo-score.json","target/jankurai/security/evidence.json","target/jankurai/security/Cargo.lock.sha256","target/jankurai/release-readiness.json"]}
JSON
printf 'artifact support ok\n'
