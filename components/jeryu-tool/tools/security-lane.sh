#!/usr/bin/env bash
set -u -o pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}" || exit 1

readonly security_script=ops/ci/security.sh
readonly gitleaks_command='gitleaks detect --source . --redact --no-banner'
if ! grep -Fq -- "$gitleaks_command" "$security_script"; then
  printf 'delegated security lane lost its exact secret-scan command\n' >&2
  exit 1
fi

status=0
bash "$security_script" "$@" || status=$?
step_status=ran
if (( status != 0 )); then
  step_status=failed
fi
printf '%s\n' \
  "jankurai-security-step={\"label\":\"jeryu-tool-security\",\"tool\":\"jeryu-tool-security\",\"shell_command\":\"bash ${security_script}\",\"status\":\"${step_status}\",\"advisory\":false,\"exit_code\":${status}}"
exit "${status}"
