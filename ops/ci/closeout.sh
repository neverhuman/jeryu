#!/usr/bin/env bash
# Canonical local closeout entrypoint.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "${ROOT}"

SUMMARY_PATH="target/ci-fast/closeout-summary.json"
mkdir -p "$(dirname "${SUMMARY_PATH}")"

set +e
JERYU_CLOSEOUT=1 \
JERYU_CLOSEOUT_SUMMARY="${SUMMARY_PATH}" \
JERYU_CI_NO_PUSH=1 \
  bash ci-fast-push.sh --full --no-push
status=$?
set -e

if [ "${status}" -ne 0 ] && [ ! -f "${SUMMARY_PATH}" ]; then
  jq -n \
    --arg schema "jeryu.closeout-summary.v1" \
    --arg status "fail" \
    --arg blocker "ci-fast-push exited before writing closeout summary" \
    --arg rerun_command "just closeout" \
    '{
      schema: $schema,
      status: $status,
      first_blocker: {
        name: $blocker,
        rerun_command: $rerun_command
      }
    }' > "${SUMMARY_PATH}"
  echo "CLOSEOUT BLOCKER: ci-fast-push exited before writing closeout summary; rerun after repair: just closeout"
fi

exit "${status}"
