#!/usr/bin/env bash
# Hostile proof for the fixed, held security-tool and advisory authorities.
set -euo pipefail

fail() {
  printf 'security lane hostile test failed: %s\n' "$*" >&2
  exit 1
}

script_path="${BASH_SOURCE[0]}"
case "${script_path}" in
  /*) ;;
  *) script_path="${PWD}/${script_path}" ;;
esac
script_dir="${script_path%/*}"
repo_root="$(cd -- "${script_dir}/.." && pwd -P)"
head="$(git -C "${repo_root}" rev-parse 'HEAD^{commit}')"
[[ -z "$(git -C "${repo_root}" status --porcelain=v1 --untracked-files=all)" ]] ||
  fail 'source checkout must be clean'

test_root="$(mktemp -d)"
case "${test_root}" in
  /tmp/tmp.*) ;;
  *) fail "mktemp returned an unexpected root: ${test_root}" ;;
esac
cleanup() {
  rm -rf -- "${test_root}"
}
trap cleanup EXIT HUP INT TERM

clone="${test_root}/repo"
git clone --quiet --no-local --no-hardlinks --no-tags "${repo_root}" "${clone}"
git -C "${clone}" checkout --quiet --detach "${head}"
[[ "$(git -C "${clone}" rev-parse 'HEAD^{commit}')" == "${head}" &&
   -z "$(git -C "${clone}" status --porcelain=v1 --untracked-files=all)" &&
   ! -e "${clone}/.git/objects/info/alternates" ]] ||
  fail 'temporary no-local exact-head clone is not isolated and clean'

shim_dir="${test_root}/path"
mkdir -- "${shim_dir}"
marker="${test_root}/forged-tool-ran"
if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
  canonical_advisory="${JAIN_RUSTSEC_ADVISORY_SOURCE:-}"
else
  canonical_advisory="$(realpath -e -- "${repo_root}/../../target/advisory-db")" ||
    fail 'canonical pinned advisory database is unavailable'
fi
[[ "${canonical_advisory}" == /* && -d "${canonical_advisory}" &&
   ! -L "${canonical_advisory}" ]] ||
  fail 'canonical pinned advisory database is not a physical absolute directory'
for name in actionlint cargo cargo-audit cargo-deny git gitleaks jq syft; do
  printf '#!/usr/bin/env bash\nprintf "%%s\\n" "%s" >>"${HOSTILE_TOOL_MARKER}"\nexit 97\n' \
    "${name}" >"${shim_dir}/${name}"
  chmod 0755 "${shim_dir}/${name}"
done

# A complete real lane run must ignore every same-name PATH executable.
HOSTILE_TOOL_MARKER="${marker}" PATH="${shim_dir}:${PATH}" \
  JAIN_RUSTSEC_ADVISORY_SOURCE="${canonical_advisory}" \
  bash -c "cd '${clone}' && bash tools/security-lane.sh" \
  >"${test_root}/stdout" 2>"${test_root}/stderr" || {
    sed -n '1,200p' "${test_root}/stderr" >&2
    fail 'security lane failed under forged PATH tools'
  }
[[ ! -e "${marker}" ]] || fail 'a forged PATH tool executed'
jq -e --arg head "${head}" '
  .schema_version == "jeryu.split.security/v3" and
  .conclusion == "success" and .git.head == $head and
  .tool_bundle.inventory_sha256 ==
    "36801d2417bbd9a804e09f30a8fd3ca96a9b5eb5e27ef9820143d44f07f4e240" and
  (.tool_bundle.tools | length) == 8 and
  .advisory.commit == "6e3286f4efa8c142fb33e5ea4342c8db6693cf34"
' "${clone}/target/security/evidence.json" >/dev/null ||
  fail 'forged-PATH run did not produce exact governed evidence'

# An imported same-name function is rejected before it can execute.
: >"${test_root}/function-log"
HOSTILE_FUNCTION_LOG="${test_root}/function-log"
export HOSTILE_FUNCTION_LOG
gitleaks() {
  printf 'forged-function\n' >>"${HOSTILE_FUNCTION_LOG}"
  return 0
}
export -f gitleaks
if bash -c "cd '${clone}' && bash tools/security-lane.sh" \
    >"${test_root}/stdout" 2>"${test_root}/stderr"; then
  unset -f gitleaks
  fail 'caller-defined gitleaks function was accepted'
fi
unset -f gitleaks
[[ ! -s "${test_root}/function-log" ]] || fail 'forged shell function executed'
grep -Fq 'rejects caller-defined shell function: gitleaks' "${test_root}/stderr" ||
  fail 'function substitution did not fail at the wrapper boundary'

# A caller cannot opt local execution into an arbitrary release-tool root.
if JAIN_RELEASE_CI=1 JAIN_NATIVE_BUILD_TOOLS_ROOT="${shim_dir}" \
    JAIN_RUSTSEC_ADVISORY_SOURCE="${test_root}/advisory" \
    bash -c "cd '${clone}' && bash tools/security-lane.sh" \
    >"${test_root}/stdout" 2>"${test_root}/stderr"; then
  fail 'caller-selected release tool root was accepted'
fi
grep -Fq 'release native tool root differs from the broker-mounted authority' \
  "${test_root}/stderr" || fail 'forged release root did not fail closed'

grep -Fq '"${TOOL_EXECS[gitleaks]}" detect --pipe' \
  "${clone}/ops/ci/security.sh" || fail 'gitleaks is not invoked by held FD'
grep -Fq '"${TOOL_EXECS[cargo-audit]}" audit' \
  "${clone}/ops/ci/security.sh" || fail 'cargo-audit is not invoked by held FD'
grep -Fq '"${TOOL_EXECS[syft]}"' \
  "${clone}/ops/ci/security.sh" || fail 'Syft is not invoked by held FD'
grep -Fq -- '--db "${advisory_db}" --no-fetch' \
  "${clone}/ops/ci/security.sh" || fail 'cargo-audit is not pinned offline'
[[ "$(find "${clone}/target/security" -mindepth 1 -maxdepth 1 -type f |
    wc -l | tr -d ' ')" == 3 ]] || fail 'security evidence set is not exactly three files'

printf 'security lane hostile test ok: head=%s\n' "${head}"
