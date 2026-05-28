#!/usr/bin/env bash
set -euo pipefail

root="${1:-.}"
ci_file="$root/.gitlab-ci.yml"

if [[ ! -f "$ci_file" ]]; then
  echo "no .gitlab-ci.yml found at $ci_file" >&2
  exit 0
fi

if grep -En '^[[:space:]]{2}tags:[[:space:]]*' "$ci_file"; then
  echo "error: standard GitLab CI jobs must not define runner tags" >&2
  echo "configure runners with run_untagged=true and an empty tag_list instead" >&2
  exit 1
fi

echo "standard GitLab CI runner-tag policy ok"
