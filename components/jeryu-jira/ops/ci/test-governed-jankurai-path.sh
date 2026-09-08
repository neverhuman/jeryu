#!/usr/bin/env bash
# Fail-closed selection tests for Jira's governed Jankurai execution path.
# Shell snippets below are deliberately single-quoted so the test shell, rather
# than this harness, expands them.
# shellcheck disable=SC2016
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../.." && pwd)"
source_lib="${here}/lib.sh"
source_verifier="${here}/ensure-jankurai.sh"
security_wrapper="${repo_root}/tools/security-lane.sh"
hosted_security="${here}/security-hosted.sh"
production_broker="/opt/jain-ci/authority/release-bin/jankurai"
production_governed="/home/ubuntu/.jeryu/bin/jankurai"
tmp="$(mktemp -d /tmp/jeryu-jira-governed-jankurai.XXXXXX)"

cleanup() {
  case "${tmp}" in
    /tmp/jeryu-jira-governed-jankurai.*) rm -rf -- "${tmp}" ;;
    *) printf 'refusing unexpected cleanup path: %s\n' "${tmp}" >&2 ;;
  esac
}
trap cleanup EXIT

fail() {
  printf 'test-governed-jankurai-path: %s\n' "$*" >&2
  exit 1
}

expect_failure() {
  local description="$1" pattern="$2"
  shift 2
  if "$@" >"${tmp}/failure.log" 2>&1; then
    fail "${description}: command unexpectedly succeeded"
  fi
  grep -Fq "${pattern}" "${tmp}/failure.log" || {
    sed -n '1,80p' "${tmp}/failure.log" >&2
    fail "${description}: expected failure text was absent"
  }
}

for source_file in "${source_lib}" "${source_verifier}"; do
  grep -Fq "${production_broker}" "${source_file}" ||
    fail "release broker path contract is absent from ${source_file}"
  grep -Fq 'local mode=receipt-bound' "${source_file}" ||
    fail "receipt-bound mode is absent from ${source_file}"
  grep -Fq "${production_governed}" "${source_file}" ||
    fail "ordinary governed path contract is absent from ${source_file}"
  grep -Fq 'governed jankurai custody mismatch: expected one link' "${source_file}" ||
    fail "single-link custody contract is absent from ${source_file}"
done

[[ "$(grep -c '^jankurai() {$' "${source_lib}")" -eq 1 ]] ||
  fail "library must define exactly one governed Jankurai wrapper"
grep -Fq 'command "${JERYU_GOVERNED_JANKURAI_BIN}" "$@"' "${source_lib}" ||
  fail "wrapper does not execute the verified governed binary"
for governed_caller in "${here}/score.sh" "${here}/proof_evidence.sh"; do
  grep -Fq 'source ops/ci/lib.sh' "${governed_caller}" ||
    fail "${governed_caller} does not source the governed library"
  grep -Fq 'require_jankurai' "${governed_caller}" ||
    fail "${governed_caller} does not verify Jankurai before use"
done
for workflow in "${repo_root}/.github/workflows/ci.yml" \
  "${repo_root}/.github/workflows/jankurai.yml"; do
  grep -Fq 'source ops/ci/lib.sh' "${workflow}" ||
    fail "${workflow} does not source the governed library"
  grep -Fq 'require_jankurai' "${workflow}" ||
    fail "${workflow} does not verify Jankurai before its audit"
  grep -Eq '^[[:space:]]+jankurai audit ' "${workflow}" ||
    fail "${workflow} lacks a visible governed Jankurai audit lane"
done
grep -Fq 'export JERYU_REQUIRE_SECURITY_TOOLS=1' "${security_wrapper}" ||
  fail "canonical security wrapper does not force strict tool custody"
grep -Fq 'exec "${system_bash}" ops/ci/security.sh "$@"' "${security_wrapper}" ||
  fail "canonical security wrapper does not exec the reviewed implementation"
grep -Fq 'exec "${system_bash}" "${wrapper}" "$@"' "${hosted_security}" ||
  fail "hosted security adapter does not exec the canonical wrapper"
grep -Fq 'bash ops/ci/security-hosted.sh' \
  "${repo_root}/.github/workflows/security.yml" ||
  fail "security workflow does not use the thin ops adapter"
jq -e '.owners["tools/"] == "ops"' agent/owner-map.json >/dev/null ||
  fail "tools owner route is missing"
jq -e '.tests["tools/"].command == "just security" and
       .tests["tools/"].lane == "security"' agent/test-map.json >/dev/null ||
  fail "tools proof route is missing"

mkdir -p "${tmp}/broker/bin" "${tmp}/attacker/bin" \
  "${tmp}/home/.jeryu/bin" "${tmp}/home/.jeryu/receipts/jankurai/sha256" \
  "${tmp}/home/.local/bin"

# shellcheck source=/dev/null
source "${source_lib}"
governed_source="${production_governed}"
governed_sha=""
if [[ ! ( "${governed_source}" == /* && -f "${governed_source}" &&
          ! -L "${governed_source}" && -x "${governed_source}" ) ]]; then
  governed_source="${production_broker}"
fi
[[ "${governed_source}" == /* && -f "${governed_source}" &&
   ! -L "${governed_source}" && -x "${governed_source}" ]] ||
  fail "governed Jankurai test source is unavailable"
[[ "$("${governed_source}" --version)" == "${JERYU_JANKURAI_VERSION}" ]] ||
  fail "governed Jankurai test source has the wrong version"
governed_sha="$(sha256sum "${governed_source}" | awk '{print $1}')"
[[ "${governed_sha}" == "${JERYU_JANKURAI_SHA256}" ]] ||
  fail "governed Jankurai test source has the wrong digest"

broker_bin="${tmp}/broker/bin/jankurai"
attacker_bin="${tmp}/attacker/bin/jankurai"
ambient_bin="${tmp}/home/.jeryu/bin/jankurai"
older_local_bin="${tmp}/home/.local/bin/jankurai"
cp -- "${governed_source}" "${broker_bin}"
cp -- "${governed_source}" "${attacker_bin}"
cp -- "${governed_source}" "${ambient_bin}"
chmod 0555 "${broker_bin}" "${attacker_bin}" "${ambient_bin}"
printf '#!/usr/bin/env bash\nprintf "hostile-path-executed\\n" >"$HOSTILE_MARKER"\nprintf "jankurai 1.6.11\\n"\n' >"${older_local_bin}"
chmod 0555 "${older_local_bin}"

# Bind the private ordinary-mode fixture to a content-addressed,
# release-authoritative installation receipt.
ordinary_receipt_tmp="${tmp}/ordinary-receipt.json"
jq -n \
  --arg remote "${JERYU_JANKURAI_SOURCE_REPO}" \
  --arg commit "${JERYU_JANKURAI_SOURCE_REV}" \
  --arg tag "${JERYU_JANKURAI_SOURCE_TAG}" \
  --arg tree "${JERYU_JANKURAI_SOURCE_TREE}" \
  --arg archive "${JERYU_JANKURAI_SOURCE_ARCHIVE_SHA256}" \
  --arg lock "${JERYU_JANKURAI_CARGO_LOCK_SHA256}" \
  --arg rustc "${JERYU_JANKURAI_RUSTC_VERSION}" \
  --arg cargo "${JERYU_JANKURAI_CARGO_VERSION}" \
  --arg triple "${JERYU_JANKURAI_TARGET_TRIPLE}" \
  --arg mode "${JERYU_JANKURAI_BUILD_MODE}" \
  --arg package_path "${JERYU_JANKURAI_PACKAGE_PATH}" \
  --arg builder_image "${JERYU_JANKURAI_BUILDER_IMAGE}" \
  --arg builder_image_id "${JERYU_JANKURAI_BUILDER_IMAGE_ID}" \
  --arg linker "${JERYU_JANKURAI_LINKER_VERSION}" \
  --arg glibc "${JERYU_JANKURAI_GLIBC_VERSION}" \
  --arg vendor "${JERYU_JANKURAI_VENDOR_FILES_SHA256}" \
  --arg vendor_count "${JERYU_JANKURAI_VENDOR_FILE_COUNT}" \
  --arg cargo_config "${JERYU_JANKURAI_CARGO_CONFIG_SHA256}" \
  --arg environment "${JERYU_JANKURAI_BUILD_ENVIRONMENT}" \
  --arg rustflags "${JERYU_JANKURAI_RUSTFLAGS}" \
  --arg command "${JERYU_JANKURAI_BUILD_COMMAND}" \
  --arg context "${JERYU_JANKURAI_BUILD_CONTEXT_SHA256}" \
  --arg digest "${JERYU_JANKURAI_SHA256}" \
  --arg version "${JERYU_JANKURAI_VERSION}" \
  --arg path "${ambient_bin}" \
  '{schema:"jeryu.jankurai-installation/v2",
    source:{remote:$remote,commit:$commit,tag:$tag,tree:$tree,
      archive_sha256:$archive,cargo_lock_sha256:$lock,
      verification:"release-authoritative"},
    build:{rustc:$rustc,cargo:$cargo,target_triple:$triple,mode:$mode,
      package_path:$package_path,builder_image:$builder_image,
      builder_image_id:$builder_image_id,linker:$linker,glibc:$glibc,
      vendor_files_sha256:$vendor,vendor_file_count:$vendor_count,
      cargo_config_sha256:$cargo_config,environment:$environment,rustflags:$rustflags,
      command:$command,context_sha256:$context,cargo_net_offline:true,
      closed_vendor:true,network_none:true,read_only_root:true,non_root:true,
      capabilities_dropped:true,no_new_privileges:true,
      container_engine_path:"/usr/bin/docker",
      git_global_config_disabled:true,git_system_config_disabled:true,
      git_http_follow_redirects:false,git_terminal_prompt:false,
      jankurai_update_check:false,
      network_scope:"local-forge-source-plus-closed-vendor-network-none",
      no_proxy:"127.0.0.1,localhost,::1"},
    governance:{status:"governed",
      manifest_repo:"http://127.0.0.1:8787/git/jeryu/jeryu-tool.git",
      manifest_commit:("a"*40),manifest_tree:("b"*40),
      manifest_sha256:("c"*64),protected_main:true,
      protection_policy:"immutable-main-v1"},
    binary:{sha256:$digest,version_output:$version},
    installation:{path:$path,atomic:true},test_mode:false,
    conclusion:"success"}' >"${ordinary_receipt_tmp}"
ordinary_receipt_sha="$(sha256sum "${ordinary_receipt_tmp}" | awk '{print $1}')"
mv -- "${ordinary_receipt_tmp}" \
  "${tmp}/home/.jeryu/receipts/jankurai/sha256/${ordinary_receipt_sha}.json"

# Only automatically removed fixtures substitute the two fixed production
# paths; the exercised selection and receipt logic remains byte-for-byte.
test_lib="${tmp}/lib.sh"
test_verifier="${tmp}/ensure-jankurai.sh"
sed -e "s#${production_broker}#${broker_bin}#g" \
  -e "s#${production_governed}#${ambient_bin}#g" \
  "${source_lib}" >"${test_lib}"
sed -e "s#${production_broker}#${broker_bin}#g" \
  -e "s#${production_governed}#${ambient_bin}#g" \
  "${source_verifier}" >"${test_verifier}"

# The library prepends and calls the held binary even when inherited PATH and
# shell-function names are hostile.
# shellcheck disable=SC2016
ordinary_command='source "$1"; require_jankurai; [[ "$JERYU_GOVERNED_JANKURAI_BIN" == "$2" ]]; [[ "$(jankurai --version)" == "$JERYU_JANKURAI_VERSION" ]]; [[ ! -e "$3" ]]'
env -i HOME="${tmp}/home" HOSTILE_MARKER="${tmp}/hostile-path-executed" \
  PATH="${tmp}/home/.local/bin:${tmp}/home/.jeryu/bin:/usr/bin:/bin" \
  bash -c "${ordinary_command}" bash "${test_lib}" "${ambient_bin}" \
  "${tmp}/hostile-path-executed"
env -i HOME="${tmp}/home" HOSTILE_MARKER="${tmp}/hostile-path-executed" \
  PATH="${tmp}/home/.local/bin:${tmp}/home/.jeryu/bin:/usr/bin:/bin" \
  bash "${test_verifier}" >/dev/null

# shellcheck disable=SC2016
function_command='jankurai() { printf hostile >"$3"; return 97; }; source "$1"; require_jankurai; [[ "$(command -v jankurai)" == jankurai ]]; [[ "$(jankurai --version)" == "$JERYU_JANKURAI_VERSION" ]]; [[ ! -e "$3" ]]'
env -i HOME="${tmp}/home" \
  PATH="${tmp}/home/.local/bin:${tmp}/home/.jeryu/bin:/usr/bin:/bin" \
  bash -c "${function_command}" bash "${test_lib}" "${ambient_bin}" \
  "${tmp}/hostile-function-executed"

expect_failure "missing ordinary auditor" \
  "governed jankurai must be an absolute executable regular file" \
  env -i PATH="/usr/bin:/bin" JERYU_GOVERNED_JANKURAI_BIN="${tmp}/missing" \
  bash -c 'source "$1"; require_jankurai' bash "${source_lib}"
ln -s "${ambient_bin}" "${tmp}/symlink-jankurai"
expect_failure "symlink ordinary auditor" \
  "governed jankurai must be an absolute executable regular file" \
  env -i PATH="/usr/bin:/bin" JERYU_GOVERNED_JANKURAI_BIN="${tmp}/symlink-jankurai" \
  bash -c 'source "$1"; require_jankurai' bash "${source_lib}"
cp -- "${ambient_bin}" "${tmp}/non-executable-jankurai"
chmod 0444 "${tmp}/non-executable-jankurai"
expect_failure "non-executable ordinary auditor" \
  "governed jankurai must be an absolute executable regular file" \
  env -i PATH="/usr/bin:/bin" JERYU_GOVERNED_JANKURAI_BIN="${tmp}/non-executable-jankurai" \
  bash -c 'source "$1"; require_jankurai' bash "${source_lib}"

ln "${ambient_bin}" "${tmp}/home/.jeryu/bin/jankurai-linked"
expect_failure "linked ordinary auditor" \
  "governed jankurai custody mismatch: expected one link" \
  env -i HOME="${tmp}/home" PATH="${tmp}/home/.jeryu/bin:/usr/bin:/bin" \
  bash -c "${ordinary_command}" bash "${test_lib}" "${ambient_bin}" \
  "${tmp}/ordinary-linked-marker"
rm -f -- "${tmp}/home/.jeryu/bin/jankurai-linked"

run_release_broker() {
  local path="$1"
  shift
  # shellcheck disable=SC2016
  env -i HOME="${tmp}/home" PATH="${path}:/usr/bin:/bin" JAIN_RELEASE_CI=1 \
    "$@" bash -c 'source "$1"; require_jankurai; [[ "$JERYU_GOVERNED_JANKURAI_BIN" == "$2" ]]; [[ "$(jankurai --version)" == "$JERYU_JANKURAI_VERSION" ]]' \
    bash "${test_lib}" "${broker_bin}"
}

run_release_broker "${tmp}/broker/bin"
env -i HOME="${tmp}/home" PATH="${tmp}/broker/bin:/usr/bin:/bin" \
  JAIN_RELEASE_CI=1 bash "${test_verifier}" >/dev/null
run_release_broker "${tmp}/broker/bin" \
  JERYU_GOVERNED_JANKURAI_BIN="${attacker_bin}" \
  JERYU_JANKURAI_BIN="${attacker_bin}"
expect_failure "caller receipt substitution" \
  "release broker Jankurai rejects caller receipt authority" \
  run_release_broker "${tmp}/broker/bin" \
  JERYU_JANKURAI_RECEIPT="${tmp}/caller-receipt.json" \
  JERYU_JANKURAI_ALLOW_TEST_RECEIPT=1
expect_failure "caller PATH substitution" "release broker Jankurai path mismatch" \
  run_release_broker "${tmp}/attacker/bin"
expect_failure "missing broker auditor" "release broker Jankurai path mismatch" \
  run_release_broker "/usr/bin:/bin"

cp -- "${broker_bin}" "${tmp}/broker-backup"
chmod 0755 "${broker_bin}"
printf '#!/usr/bin/env bash\nprintf "jankurai 1.6.11\\n"\n' >"${broker_bin}"
chmod 0555 "${broker_bin}"
expect_failure "wrong broker digest" "governed jankurai identity mismatch" \
  run_release_broker "${tmp}/broker/bin"
rm -f -- "${broker_bin}"
mv -- "${tmp}/broker-backup" "${broker_bin}"
chmod 0755 "${broker_bin}"
expect_failure "writable broker binary" "release broker Jankurai custody mismatch" \
  run_release_broker "${tmp}/broker/bin"
chmod 0555 "${broker_bin}"
ln "${broker_bin}" "${tmp}/broker/bin/jankurai-linked"
expect_failure "linked broker binary" "release broker Jankurai custody mismatch" \
  run_release_broker "${tmp}/broker/bin"
rm -f -- "${tmp}/broker/bin/jankurai-linked"

printf 'governed Jankurai path tests passed: function path receipt symlink mode digest hardlink broker\n'
