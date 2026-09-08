#!/usr/bin/env bash
# Fail-closed source, dependency, workflow, advisory, and SBOM gate.
set -euo pipefail
set +o xtrace
set +o verbose
shopt -u varredir_close 2>/dev/null || true

readonly SOURCE_NAME='jeryu-cache'
readonly RELEASE_VERSION='jeryu-cache-v5.0.0-split.2'
readonly TOOL_BUNDLE_ID='36801d2417bbd9a804e09f30a8fd3ca96a9b5eb5e27ef9820143d44f07f4e240'
readonly LOCAL_TOOL_ROOT="/var/lib/jain-host-ci/native-build-tools/${TOOL_BUNDLE_ID}"
readonly RELEASE_TOOL_ROOT="/opt/jain-ci/native-build-tools/${TOOL_BUNDLE_ID}"
readonly ADVISORY_COMMIT='6e3286f4efa8c142fb33e5ea4342c8db6693cf34'
readonly ADVISORY_TREE='d12220aff0053a035739bec6e64aefbaafbf01a3'

declare -Ar TOOL_SHA256=(
  [actionlint]='9ab20f97947e525d92175a7029eba4fe62749b49e556a735862dac037dd6f8dd'
  [cargo]='f30f9fd1b1d0b8fd10dc33219eb4cd4bec3543f40e434ac71f5a03fd0359063f'
  [cargo-audit]='1a17ff4c0449d1924aacda8dd20c06dccc3cceeed4dd17a71523f672bf97b70b'
  [cargo-deny]='ef27c757f50d77c5c2d9114fbc6ad45d2b8903506cead473a70b8ee659ea7a18'
  [git]='2a8c18fbf43da9f692d75474c72bea9dfd796c260b0f3dfe456376abc3bbd668'
  [gitleaks]='50b742abd7daad8bbddb6301f3017efb680632d9a5b3b4d8f137b3aac250e359'
  [jq]='59cfd58d7e470b103aede0e7589cfea929e45ee27f5471f08aa9676ac7bfc566'
  [syft]='eb9714fb8e4b8f2a647e7bb312f1e0b9f83a7aa30418658bf46583cfa83d27d2'
)
readonly -a TOOL_NAMES=(
  actionlint cargo cargo-audit cargo-deny git gitleaks jq syft
)
declare -A TOOL_FDS=()
declare -A TOOL_EXECS=()
declare -A TOOL_IDENTITIES=()
declare -A TOOL_PATHS=()

die() {
  printf 'security check failed: %s\n' "$*" >&2
  exit 1
}

# Exported Bash functions take precedence over PATH commands. Reject them even
# though every security-sensitive executable below is addressed by a held FD;
# this keeps direct invocation as strict as the canonical wrapper.
reject_shell_functions() {
  local name
  for name in actionlint awk cargo cargo-audit cargo-deny cat chmod cp find \
      git gitleaks grep jq mkdir mktemp mv realpath rm sha256sum sort stat syft; do
    if declare -F -- "${name}" >/dev/null 2>&1; then
      die "caller-defined shell function is forbidden: ${name}"
    fi
  done
}

physical_directory() {
  local path="$1" label="$2" resolved custody
  [[ "${path}" == /* && -d "${path}" && ! -L "${path}" ]] ||
    die "${label} is not a physical absolute directory: ${path}"
  resolved="$(/usr/bin/realpath -e -- "${path}" 2>/dev/null || true)"
  [[ "${resolved}" == "${path}" ]] ||
    die "${label} traverses a symlink: ${path}"
  custody="$(/usr/bin/stat -Lc '%u:%g:%a' -- "${path}")"
  [[ "${custody}" == '0:0:555' && ! -w "${path}" ]] ||
    die "${label} lacks root-owned mode 0555 custody: ${custody}"
}

require_read_only_mount() {
  local path="$1" options
  options="$(/usr/bin/findmnt -rn -o OPTIONS --target "${path}" 2>/dev/null || true)"
  [[ ",${options}," == *,ro,* ]] ||
    die "release tool bundle is not mounted read-only: ${path}"
}

sha256_file() {
  local record
  record="$(/usr/bin/sha256sum -- "$1")" || return 1
  printf '%s\n' "${record%% *}"
}

assert_tool_stable() {
  local name="$1" path fd_path fd_identity path_identity digest
  path="${TOOL_PATHS[${name}]}"
  fd_path="/proc/self/fd/${TOOL_FDS[${name}]}"
  [[ "${path}" == "${tool_bin}/${name}" && -f "${path}" && ! -L "${path}" &&
     -x "${path}" && "$(/usr/bin/realpath -e -- "${path}" 2>/dev/null || true)" == "${path}" ]] ||
    die "canonical tool path changed or became non-physical: ${name}"
  fd_identity="$(/usr/bin/stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "${fd_path}")"
  path_identity="$(/usr/bin/stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "${path}")"
  digest="$(sha256_file "${fd_path}")"
  [[ "${fd_identity}" == "${TOOL_IDENTITIES[${name}]}" &&
     "${path_identity}" == "${TOOL_IDENTITIES[${name}]}" &&
     "${digest}" == "${TOOL_SHA256[${name}]}" ]] ||
    die "held tool identity changed or canonical path was swapped: ${name}"
  [[ "$(/usr/bin/stat -Lc '%u:%g:%a:%h' -- "${path}")" == '0:0:555:1' &&
     ! -w "${path}" ]] ||
    die "tool lost root-owned immutable custody: ${name}"
}

assert_all_tools_stable() {
  local name
  physical_directory "${tool_root}" 'native tool bundle root'
  physical_directory "${tool_bin}" 'native tool bundle bin root'
  if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
    require_read_only_mount "${tool_root}"
  fi
  for name in "${TOOL_NAMES[@]}"; do
    assert_tool_stable "${name}"
  done
}

hold_tool() {
  local name="$1" path fd identity kind links
  path="${tool_bin}/${name}"
  [[ -f "${path}" && ! -L "${path}" && -x "${path}" &&
     "$(/usr/bin/realpath -e -- "${path}" 2>/dev/null || true)" == "${path}" ]] ||
    die "required tool is not a physical executable: ${path}"
  [[ "$(/usr/bin/stat -Lc '%u:%g:%a:%h' -- "${path}")" == '0:0:555:1' &&
     ! -w "${path}" ]] ||
    die "required tool is not root-owned immutable custody: ${path}"
  exec {fd}<"${path}" || die "cannot hold required tool: ${path}"
  identity="$(/usr/bin/stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "/proc/self/fd/${fd}")"
  IFS='|' read -r kind links _ <<<"${identity}"
  [[ "${kind}" == 'regular file' && "${links}" == 1 &&
     "${identity}" == "$(/usr/bin/stat -Lc '%F|%h|%d|%i|%s|%u|%g|%a|%Y|%Z' -- "${path}")" &&
     "$(sha256_file "/proc/self/fd/${fd}")" == "${TOOL_SHA256[${name}]}" ]] ||
    die "required tool digest or held identity differs: ${name}"
  TOOL_FDS["${name}"]="${fd}"
  TOOL_EXECS["${name}"]="/proc/self/fd/${fd}"
  TOOL_IDENTITIES["${name}"]="${identity}"
  TOOL_PATHS["${name}"]="${path}"
}

closed_tool() {
  /usr/bin/env -i \
    HOME="${trusted_home}" \
    PATH="${tool_bin}:/usr/bin:/bin" \
    LANG=C LC_ALL=C TZ=UTC \
    CARGO_HOME="${trusted_cargo_home}" \
    CARGO_TARGET_DIR="${trusted_target_dir}" \
    CARGO_NET_OFFLINE=true CARGO_TERM_COLOR=never \
    RUSTUP_HOME="${trusted_rustup_home}" \
    SYFT_CHECK_FOR_APP_UPDATE=false \
    "$@"
}

governed_git() {
  /usr/bin/env -i \
    HOME=/nonexistent PATH="${tool_bin}:/usr/bin:/bin" LANG=C LC_ALL=C \
    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_SYSTEM=/dev/null \
    GIT_CONFIG_GLOBAL=/dev/null GIT_ATTR_NOSYSTEM=1 \
    GIT_NO_REPLACE_OBJECTS=1 \
    "${TOOL_EXECS[git]}" --no-replace-objects "$@"
}

governed_advisory_git() {
  governed_git -c "safe.directory=${advisory_db}" -C "${advisory_db}" "$@"
}

ensure_physical_output_directory() {
  local path="$1" label="$2" resolved
  if [[ -e "${path}" || -L "${path}" ]]; then
    [[ -d "${path}" && ! -L "${path}" ]] ||
      die "${label} is not a physical directory"
  else
    "${tool_bin}/mkdir" -- "${path}"
  fi
  resolved="$("${tool_bin}/realpath" -e -- "${path}" 2>/dev/null || true)"
  [[ "${resolved}" == "${path}" ]] || die "${label} traverses a symlink"
}

reject_shell_functions

script_path="${BASH_SOURCE[0]}"
case "${script_path}" in
  /*) ;;
  *) script_path="${PWD}/${script_path}" ;;
esac
script_dir="${script_path%/*}"
root="$(cd -- "${script_dir}/../.." && pwd -P)"
family_root="$(cd -- "${root}/../.." && pwd -P)"
cd "${root}"

case "${JAIN_RELEASE_CI:-0}" in
  0)
    tool_root="${LOCAL_TOOL_ROOT}"
    tool_custody='local-root-owned-native-build-tools-v2'
    trusted_home="$(/usr/bin/getent passwd "${EUID}" | /usr/bin/cut -d: -f6)"
    [[ "${trusted_home}" == /* && -d "${trusted_home}" && ! -L "${trusted_home}" &&
       "$(/usr/bin/realpath -e -- "${trusted_home}" 2>/dev/null || true)" == "${trusted_home}" ]] ||
      die 'cannot derive a physical passwd home for local security execution'
    trusted_cargo_home="${CARGO_HOME:-${trusted_home}/.cargo}"
    trusted_rustup_home="${RUSTUP_HOME:-${trusted_home}/.rustup}"
    trusted_target_dir="${CARGO_TARGET_DIR:-${root}/target}"
    advisory_db="${JAIN_RUSTSEC_ADVISORY_SOURCE:-${family_root}/target/advisory-db}"
    ;;
  1)
    [[ "${JAIN_NATIVE_BUILD_TOOLS_ROOT:-}" == "${RELEASE_TOOL_ROOT}" ]] ||
      die 'release native tool root differs from the broker-mounted authority'
    tool_root="${RELEASE_TOOL_ROOT}"
    tool_custody='release-read-only-native-build-tools-v2'
    trusted_home="${HOME:-/nonexistent}"
    trusted_cargo_home="${CARGO_HOME:-}"
    trusted_rustup_home="${RUSTUP_HOME:-}"
    trusted_target_dir="${CARGO_TARGET_DIR:-}"
    advisory_db="${JAIN_RUSTSEC_ADVISORY_SOURCE:-}"
    [[ "${trusted_home}" == /* && "${trusted_cargo_home}" == /* &&
       "${trusted_rustup_home}" == /* && "${trusted_target_dir}" == /* &&
       "${advisory_db}" == '/opt/jain-ci/authority/advisory-db' ]] ||
      die 'release Cargo, Rustup, target, home, or advisory paths lack broker authority'
    ;;
  *) die 'JAIN_RELEASE_CI must be 0 or 1' ;;
esac

tool_bin="${tool_root}/bin"
physical_directory "${tool_root}" 'native tool bundle root'
physical_directory "${tool_bin}" 'native tool bundle bin root'
if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
  require_read_only_mount "${tool_root}"
fi
for tool_name in "${TOOL_NAMES[@]}"; do
  hold_tool "${tool_name}"
done
assert_all_tools_stable

[[ "$(closed_tool "${TOOL_EXECS[cargo]}" --version)" == \
   'cargo 1.96.0 (30a34c682 2026-05-25)' ]] ||
  die 'cargo version differs from the governed bundle'
[[ "$(closed_tool "${TOOL_EXECS[cargo-audit]}" --version)" == \
   'cargo-audit 0.22.1' ]] ||
  die 'cargo-audit version differs from the governed bundle'
[[ "$(closed_tool "${TOOL_EXECS[cargo-deny]}" --version)" == \
   'cargo-deny 0.19.8' ]] ||
  die 'cargo-deny version differs from the governed bundle'
[[ "$(closed_tool "${TOOL_EXECS[gitleaks]}" version)" == '8.21.2' ]] ||
  die 'gitleaks version differs from the governed bundle'
actionlint_version="$(closed_tool "${TOOL_EXECS[actionlint]}" --version 2>&1)"
[[ "${actionlint_version%%$'\n'*}" == '1.7.8' ]] ||
  die 'actionlint version differs from the governed bundle'
syft_version="$(closed_tool "${TOOL_EXECS[syft]}" version -o json |
  "${TOOL_EXECS[jq]}" -er 'select(.application == "syft") | .version')"
[[ "${syft_version}" == '1.40.0' ]] ||
  die 'Syft version differs from the governed bundle'
[[ "$(governed_git --version)" == 'git version 2.43.0' ]] ||
  die 'Git version differs from the governed bundle'
[[ "$(closed_tool "${TOOL_EXECS[jq]}" --version)" == 'jq-1.7' ]] ||
  die 'jq version differs from the governed bundle'
assert_all_tools_stable

head_sha="$(governed_git -C "${root}" rev-parse 'HEAD^{commit}')"
tree_sha="$(governed_git -C "${root}" rev-parse 'HEAD^{tree}')"
[[ "${head_sha}" =~ ^[0-9a-f]{40}$ && "${tree_sha}" =~ ^[0-9a-f]{40}$ ]] ||
  die 'cannot resolve the exact source head and tree'
[[ -z "$(governed_git -C "${root}" status --porcelain=v1 --untracked-files=all)" ]] ||
  die 'source must be clean, including untracked paths, before security evidence generation'
for source_path in ops/ci/lib.sh ops/ci/security.sh tools/security-lane.sh VERSION Cargo.lock; do
  expected_blob="$(governed_git -C "${root}" rev-parse "${head_sha}:${source_path}" 2>/dev/null || true)"
  actual_blob="$(governed_git -C "${root}" hash-object --no-filters -- "${root}/${source_path}" 2>/dev/null || true)"
  [[ "${expected_blob}" =~ ^[0-9a-f]{40}$ && "${actual_blob}" == "${expected_blob}" ]] ||
    die "security authority source differs from HEAD: ${source_path}"
done

# The shared source validator is itself bound above before it is sourced.
# shellcheck source=ops/ci/lib.sh
source ops/ci/lib.sh
jeryu_assert_closed_source_authority "${root}" "${head_sha}" 'security source checkout' ||
  die 'closed physical source authority validation failed'

[[ -f VERSION && ! -L VERSION && "$("${tool_bin}/stat" -c '%h' -- VERSION)" == 1 ]] ||
  die 'VERSION must be a one-link regular file'
source_version="$(<VERSION)"
[[ "${source_version}" == "${RELEASE_VERSION}" ]] ||
  die "VERSION must name the governed successor ${RELEASE_VERSION}"

[[ "${advisory_db}" == /* && -d "${advisory_db}" && ! -L "${advisory_db}" &&
   "$("${tool_bin}/realpath" -e -- "${advisory_db}" 2>/dev/null || true)" == "${advisory_db}" &&
   -d "${advisory_db}/.git" && ! -L "${advisory_db}/.git" ]] ||
  die 'pinned RustSec advisory database is not a physical standalone checkout'
advisory_head="$(governed_advisory_git rev-parse 'HEAD^{commit}')"
advisory_tree="$(governed_advisory_git rev-parse 'HEAD^{tree}')"
[[ "${advisory_head}" == "${ADVISORY_COMMIT}" &&
   "${advisory_tree}" == "${ADVISORY_TREE}" &&
   -z "$(governed_advisory_git status --porcelain=v1 --untracked-files=all)" ]] ||
  die 'RustSec advisory database differs from the pinned clean commit and tree'

ensure_physical_output_directory "${root}/target" 'target root'
ensure_physical_output_directory "${root}/target/security" 'security evidence root'
ensure_physical_output_directory "${root}/target/jankurai" 'Jankurai evidence parent'
ensure_physical_output_directory "${root}/target/jankurai/security" 'Jankurai security evidence root'
for output in target/security/cargo-audit.json \
    target/security/jeryu-cache.spdx.json target/security/evidence.json \
    target/jankurai/security/source-security-evidence.json; do
  if [[ -e "${output}" || -L "${output}" ]]; then
    [[ -f "${output}" && ! -L "${output}" &&
       "$("${tool_bin}/stat" -c '%h' -- "${output}")" == 1 ]] ||
      die "${output} lacks regular single-link custody"
  fi
done

audit_tmp="$("${tool_bin}/mktemp" "${root}/target/security/.cargo-audit.XXXXXX")"
sbom_tmp="$("${tool_bin}/mktemp" "${root}/target/security/.sbom.XXXXXX")"
evidence_tmp="$("${tool_bin}/mktemp" "${root}/target/security/.evidence.XXXXXX")"
cleanup() {
  "${tool_bin}/rm" -f -- "${audit_tmp:-}" "${sbom_tmp:-}" "${evidence_tmp:-}"
}
trap cleanup EXIT HUP INT TERM

{
  governed_git -C "${root}" ls-files -z
  governed_git -C "${root}" ls-files --others --exclude-standard -z
} | "${tool_bin}/sort" -zu | while IFS= read -r -d '' path; do
  [[ -f "${path}" ]] || continue
  case "${path}" in
    target/*|.jankurai/*|agent/repo-score.json|agent/repo-score.md)
      continue
      ;;
  esac
  if LC_ALL=C "${tool_bin}/grep" -Iq . "${path}"; then
    printf '\n===== %s =====\n' "${path}"
    "${tool_bin}/cat" "${path}"
  fi
done | closed_tool "${TOOL_EXECS[gitleaks]}" detect --pipe --redact --verbose
assert_all_tools_stable

if [[ -d .github/workflows ]]; then
  closed_tool "${TOOL_EXECS[actionlint]}" .github/workflows/*.yml
fi
if "${tool_bin}/find" . -path './.git' -prune -o -path './target' -prune -o \
    -path './.jankurai' -prune -o -name '.env' -type f -print |
    "${tool_bin}/grep" -q .; then
  die 'repository contains a .env file'
fi

closed_tool "${TOOL_EXECS[cargo]}" metadata --locked --offline \
  --format-version 1 --no-deps >/dev/null
closed_tool "${TOOL_EXECS[cargo-deny]}" check bans licenses sources --disable-fetch
if ! closed_tool "${TOOL_EXECS[cargo-audit]}" audit \
    --db "${advisory_db}" --no-fetch --deny warnings --json >"${audit_tmp}"; then
  die 'cargo audit reported advisories or could not consume the pinned database'
fi
"${TOOL_EXECS[jq]}" -e 'type == "object"' "${audit_tmp}" >/dev/null ||
  die 'cargo audit did not emit a JSON object'
assert_all_tools_stable

if ! closed_tool "${TOOL_EXECS[syft]}" \
    scan dir:. --source-name "${SOURCE_NAME}" --source-version "${source_version}" \
    --exclude './target/**' --exclude './.git/**' --exclude './.jankurai/**' \
    --exclude './agent/repo-score.json' --exclude './agent/repo-score.md' \
    --output "spdx-json=${sbom_tmp}" >/dev/null; then
  die 'Syft scan failed'
fi
"${TOOL_EXECS[jq]}" -e --arg name "${SOURCE_NAME}" --arg version "${source_version}" '
  .spdxVersion == "SPDX-2.3" and .name == $name and
  (.creationInfo.creators | index("Tool: syft-1.40.0")) != null and
  (.packages | type == "array" and length > 0) and
  ([.packages[] | select(
    .name == $name and .versionInfo == $version and
    .SPDXID == ("SPDXRef-DocumentRoot-Directory-" + $name) and
    .primaryPackagePurpose == "FILE"
  )] | length) == 1
' "${sbom_tmp}" >/dev/null ||
  die 'SBOM does not bind the expected source name and version'
assert_all_tools_stable

[[ "$(governed_git -C "${root}" rev-parse 'HEAD^{commit}')" == "${head_sha}" &&
   "$(governed_git -C "${root}" rev-parse 'HEAD^{tree}')" == "${tree_sha}" &&
   -z "$(governed_git -C "${root}" status --porcelain=v1 --untracked-files=all)" ]] ||
  die 'source moved during security evidence generation'
[[ "$(governed_advisory_git rev-parse 'HEAD^{commit}')" == "${ADVISORY_COMMIT}" &&
   "$(governed_advisory_git rev-parse 'HEAD^{tree}')" == "${ADVISORY_TREE}" &&
   -z "$(governed_advisory_git status --porcelain=v1 --untracked-files=all)" ]] ||
  die 'advisory authority moved during security evidence generation'

audit_sha="$(sha256_file "${audit_tmp}")"
sbom_sha="$(sha256_file "${sbom_tmp}")"
lock_sha="$(sha256_file Cargo.lock)"
"${TOOL_EXECS[jq]}" -nS \
  --arg head "${head_sha}" --arg tree "${tree_sha}" \
  --arg source_name "${SOURCE_NAME}" --arg source_version "${source_version}" \
  --arg lock_sha256 "${lock_sha}" --arg audit_sha256 "${audit_sha}" \
  --arg sbom_sha256 "${sbom_sha}" \
  --arg bundle_id "${TOOL_BUNDLE_ID}" --arg bundle_root "${tool_root}" \
  --arg bundle_custody "${tool_custody}" \
  --arg advisory_path "${advisory_db}" \
  --arg advisory_commit "${ADVISORY_COMMIT}" --arg advisory_tree "${ADVISORY_TREE}" \
  --arg actionlint_path "${TOOL_PATHS[actionlint]}" --arg actionlint_sha "${TOOL_SHA256[actionlint]}" \
  --arg cargo_path "${TOOL_PATHS[cargo]}" --arg cargo_sha "${TOOL_SHA256[cargo]}" \
  --arg audit_path "${TOOL_PATHS[cargo-audit]}" --arg audit_tool_sha "${TOOL_SHA256[cargo-audit]}" \
  --arg deny_path "${TOOL_PATHS[cargo-deny]}" --arg deny_sha "${TOOL_SHA256[cargo-deny]}" \
  --arg git_path "${TOOL_PATHS[git]}" --arg git_sha "${TOOL_SHA256[git]}" \
  --arg gitleaks_path "${TOOL_PATHS[gitleaks]}" --arg gitleaks_sha "${TOOL_SHA256[gitleaks]}" \
  --arg jq_path "${TOOL_PATHS[jq]}" --arg jq_sha "${TOOL_SHA256[jq]}" \
  --arg syft_path "${TOOL_PATHS[syft]}" --arg syft_sha "${TOOL_SHA256[syft]}" '
  {schema_version:"jeryu.split.security/v3",conclusion:"success",
   git:{head:$head,tree:$tree,dirty_worktree:false},
   source:{name:$source_name,version:$source_version},
   cargo_lock_sha256:$lock_sha256,
   advisory:{path:$advisory_path,commit:$advisory_commit,tree:$advisory_tree,
     clean:true,network_fetch:false},
   tool_bundle:{inventory_sha256:$bundle_id,root:$bundle_root,custody:$bundle_custody,
     tools:{
       actionlint:{path:$actionlint_path,sha256:$actionlint_sha,version:"1.7.8"},
       cargo:{path:$cargo_path,sha256:$cargo_sha,version:"cargo 1.96.0 (30a34c682 2026-05-25)"},
       "cargo-audit":{path:$audit_path,sha256:$audit_tool_sha,version:"cargo-audit 0.22.1"},
       "cargo-deny":{path:$deny_path,sha256:$deny_sha,version:"cargo-deny 0.19.8"},
       git:{path:$git_path,sha256:$git_sha,version:"git version 2.43.0"},
       gitleaks:{path:$gitleaks_path,sha256:$gitleaks_sha,version:"8.21.2"},
       jq:{path:$jq_path,sha256:$jq_sha,version:"jq-1.7"},
       syft:{path:$syft_path,sha256:$syft_sha,version:"1.40.0"}}},
   checks:["root-owned-held-tool-bundle","gitleaks-8.21.2","actionlint-1.7.8",
     "env-file-absence","cargo-metadata-locked-offline","cargo-deny-0.19.8",
     "cargo-audit-0.22.1-pinned-no-fetch","syft-1.40.0-spdx"],
   artifacts:{
     cargo_audit:{path:"target/security/cargo-audit.json",sha256:$audit_sha256},
     sbom:{path:"target/security/jeryu-cache.spdx.json",sha256:$sbom_sha256}}}
' >"${evidence_tmp}"

"${tool_bin}/chmod" 0600 "${audit_tmp}" "${sbom_tmp}" "${evidence_tmp}"
[[ "$("${tool_bin}/stat" -c '%h' -- "${audit_tmp}")" == 1 &&
   "$("${tool_bin}/stat" -c '%h' -- "${sbom_tmp}")" == 1 &&
   "$("${tool_bin}/stat" -c '%h' -- "${evidence_tmp}")" == 1 ]] ||
  die 'temporary evidence is multiply linked'
[[ "$(sha256_file "${audit_tmp}")" == "${audit_sha}" &&
   "$(sha256_file "${sbom_tmp}")" == "${sbom_sha}" ]] ||
  die 'evidence moved before publication'
assert_all_tools_stable

"${tool_bin}/mv" -- "${audit_tmp}" target/security/cargo-audit.json
audit_tmp=''
"${tool_bin}/mv" -- "${sbom_tmp}" target/security/jeryu-cache.spdx.json
sbom_tmp=''
"${tool_bin}/mv" -- "${evidence_tmp}" target/security/evidence.json
evidence_tmp=''
"${tool_bin}/cp" --reflink=never target/security/evidence.json \
  target/jankurai/security/source-security-evidence.json

[[ "$(governed_git -C "${root}" rev-parse 'HEAD^{commit}')" == "${head_sha}" &&
   "$(governed_git -C "${root}" rev-parse 'HEAD^{tree}')" == "${tree_sha}" &&
   -z "$(governed_git -C "${root}" status --porcelain=v1 --untracked-files=all)" ]] ||
  die 'source moved before final security readback'
assert_all_tools_stable
"${TOOL_EXECS[jq]}" -e --arg head "${head_sha}" --arg tree "${tree_sha}" \
  --arg bundle "${TOOL_BUNDLE_ID}" --arg advisory "${ADVISORY_COMMIT}" '
  .schema_version == "jeryu.split.security/v3" and .conclusion == "success" and
  .git.head == $head and .git.tree == $tree and .git.dirty_worktree == false and
  .tool_bundle.inventory_sha256 == $bundle and
  (.tool_bundle.tools | keys == ["actionlint","cargo","cargo-audit","cargo-deny",
    "git","gitleaks","jq","syft"]) and
  all(.tool_bundle.tools[]; .path | startswith("/")) and
  all(.tool_bundle.tools[]; .sha256 | test("^[0-9a-f]{64}$")) and
  .advisory.commit == $advisory and .advisory.clean == true and
  .advisory.network_fetch == false and (.checks | length) == 8
' target/security/evidence.json >/dev/null ||
  die 'published security evidence failed readback'
[[ "$(sha256_file target/security/evidence.json)" == \
   "$(sha256_file target/jankurai/security/source-security-evidence.json)" ]] ||
  die 'Jankurai security evidence copy differs from the canonical receipt'
trap - EXIT HUP INT TERM
printf 'security ok: head=%s tree=%s tools=%s advisory=%s\n' \
  "${head_sha}" "${tree_sha}" "${TOOL_BUNDLE_ID}" "${ADVISORY_COMMIT}"
