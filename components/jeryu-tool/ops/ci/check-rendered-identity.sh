#!/usr/bin/env bash
# CI explicitly selects candidate rendering; the protected renderer stays separate.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
tool_root=$(cd -- "$here/../.." && pwd -P)
if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 0 ]]; then
  exec bash "$tool_root/ops/render-tool-manifest.sh" "$@"
fi
[[ ${JERYU_MONOREPO_CANDIDATE} == 1 && ${JAIN_RELEASE_CI:-0} != 1 ]] || {
  printf 'candidate rendering cannot satisfy protected release authority\n' >&2; exit 1;
}
monorepo_root=$(git -C "$tool_root" rev-parse --show-toplevel 2>/dev/null) || {
  printf 'candidate rendering cannot run without a git checkout\n' >&2; exit 1;
}
monorepo_root=$(cd -- "$monorepo_root" && pwd -P)
if [[ $tool_root != "$monorepo_root/components/jeryu-tool" && $tool_root != "$monorepo_root" ]]; then
  printf 'candidate rendering requires the Tool component checkout\n' >&2; exit 1;
fi
[[ $tool_root == "$monorepo_root/components/jeryu-tool" ]] || [[ $tool_root == "$monorepo_root" ]]
declare -A selected=() overrides=()
check=0
while [[ $# -gt 0 ]]; do
  case $1 in
    --check) [[ $check == 0 ]]; check=1; shift ;;
    --repo|--repo-root)
      flag=$1
      [[ $# -ge 2 ]]
      value=$2
      name=${value%%=*}
      case $name in
        jeryu|jeryu-cache|jeryu-ci-runner|jeryu-core|jeryu-deploy|jeryu-intelligence|jeryu-jira|jeryu-release-ops|jeryu-tool|jeryu-tool-finder|jeryu-web) ;;
        *) printf 'unknown candidate consumer\n' >&2; exit 1 ;;
      esac
      if [[ $flag == --repo ]]; then
        [[ $value == "$name" && ! -v selected[$name] ]]
        selected[$name]=1
      else
        [[ $value == *=* && ! -v overrides[$name] ]]
        path=${value#*=}
        expected="$monorepo_root/components/$name"
        [[ $name != jeryu ]] || expected=$monorepo_root
        [[ $path == "$expected" && $(realpath -e -- "$path") == "$expected" ]]
        overrides[$name]=1
      fi
      shift 2
      ;;
    *) printf 'candidate CI supports only read-only consumer checks\n' >&2; exit 1 ;;
  esac
done
[[ $check == 1 ]]
for name in "${!overrides[@]}"; do [[ -v selected[$name] ]]; done
# A component request still qualifies all generated consumers at the exact source.
exec bash "$tool_root/ops/render-monorepo-candidate.sh" --check \
  --expected-head "${JERYU_MONOREPO_EXPECTED_HEAD:?exact candidate head is required}"
