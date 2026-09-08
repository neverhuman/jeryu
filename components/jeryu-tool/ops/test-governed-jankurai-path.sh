#!/usr/bin/env bash
# Fail-closed selection tests for the release broker's governed Jankurai path.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source_lib="${here}/ci/lib.sh"
# shellcheck source=ops/ci/lib.sh
source "${source_lib}"
production_broker="/opt/jain-ci/authority/release-bin/jankurai"
production_governed="/home/ubuntu/.jeryu/bin/jankurai"
tmp="$(mktemp -d /tmp/test-governed-jankurai-path.XXXXXX)"
source_verifier="${tmp}/ensure-jankurai.sh"
cleanup() {
  rm -rf -- "${tmp}"
}
trap cleanup EXIT

repo_root="$(cd "${here}/.." && pwd)"
cargo run --quiet --locked --offline --manifest-path "${repo_root}/Cargo.toml" \
  --bin jeryu-toolctl -- --tool-root "${repo_root}" emit-ensure-script \
  >"${source_verifier}"

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

for source in "${source_lib}" "${source_verifier}"; do
  grep -Fq "${production_broker}" "${source}" ||
    fail "release broker path contract is absent from ${source}"
  grep -Fq 'local mode=receipt-bound' "${source}" ||
    fail "release broker mode is absent from ${source}"
  grep -Fq "${production_governed}" "${source}" ||
    fail "ordinary governed path contract is absent from ${source}"
  grep -Fq 'governed jankurai custody mismatch: expected one link' "${source}" ||
    fail "ordinary single-link custody contract is absent from ${source}"
done
[[ "$(grep -c '^jankurai() {$' "${source_lib}")" -eq 1 ]] ||
  fail "consumer library must define exactly one governed Jankurai wrapper"
grep -Fq 'command "${JERYU_GOVERNED_JANKURAI_BIN}" "$@"' "${source_lib}" ||
  fail "consumer wrapper does not execute the verified governed binary"
if grep -Eq '/home/ubuntu/\.jeryu/bin/jankurai|JERYU_JANKURAI_BIN:-' \
  "${here}/ci/pr-ci.sh"; then
  fail "PR gate still selects an ambient-home or caller-provided auditor"
fi

mkdir -p "${tmp}/broker/bin" "${tmp}/attacker/bin" \
  "${tmp}/home/.jeryu/bin" "${tmp}/home/.jeryu/receipts/jankurai/sha256" \
  "${tmp}/home/.local/bin"
governed_source="${production_governed}"
if [[ ! ( "${governed_source}" == /* && -f "${governed_source}" &&
          ! -L "${governed_source}" && -x "${governed_source}" ) ]]; then
  governed_source="${production_broker}"
fi
[[ "${governed_source}" == /* && -f "${governed_source}" &&
   ! -L "${governed_source}" && -x "${governed_source}" ]] ||
  fail "governed Jankurai test source is unavailable"
[[ "$("${governed_source}" --version)" == 'jankurai 1.6.11' ]] ||
  fail "governed Jankurai test source has the wrong version"
[[ "$(sha256sum "${governed_source}" | awk '{print $1}')" == \
   "${JERYU_JANKURAI_SHA256}" ]] ||
  fail "governed Jankurai test source has the wrong digest"

broker_bin="${tmp}/broker/bin/jankurai"
attacker_bin="${tmp}/attacker/bin/jankurai"
ambient_bin="${tmp}/home/.jeryu/bin/jankurai"
older_local_bin="${tmp}/home/.local/bin/jankurai"
cp -- "${governed_source}" "${broker_bin}"
cp -- "${governed_source}" "${attacker_bin}"
cp -- "${governed_source}" "${ambient_bin}"
chmod 0555 "${broker_bin}" "${attacker_bin}" "${ambient_bin}"
printf '#!/usr/bin/env bash\nprintf "jankurai 1.6.11\\n"\n' >"${older_local_bin}"
chmod 0555 "${older_local_bin}"

# Bind the private ordinary-mode fixture to a release-authoritative receipt.
# The content-addressed filename is the receipt's own digest.
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

# Exercise the exact production logic without requiring write access beneath
# /opt: only this automatically removed test copy substitutes the fixed broker
# path, while every other byte remains the reviewed source.
test_lib="${tmp}/lib.sh"
sed -e "s#${production_broker}#${broker_bin}#g" \
  -e "s#${production_governed}#${ambient_bin}#g" \
  "${source_lib}" >"${test_lib}"
test_verifier="${tmp}/test-ensure-jankurai.sh"
sed -e "s#${production_broker}#${broker_bin}#g" \
  -e "s#${production_governed}#${ambient_bin}#g" \
  "${source_verifier}" >"${test_verifier}"

# Ordinary mode ignores the older ~/.local/bin candidate, selects the governed
# installation, and proves its release-authoritative receipt.
# The child shell expands its positional inputs.
# shellcheck disable=SC2016
ordinary_command='source "$1"; require_jankurai; [[ "$JERYU_GOVERNED_JANKURAI_BIN" == "$2" ]]'
env -i HOME="${tmp}/home" \
  PATH="${tmp}/home/.local/bin:${tmp}/home/.jeryu/bin:/usr/bin:/bin" \
  bash -c "${ordinary_command}" bash "${test_lib}" "${ambient_bin}"
env -i HOME="${tmp}/home" \
  PATH="${tmp}/home/.local/bin:${tmp}/home/.jeryu/bin:/usr/bin:/bin" \
  bash "${test_verifier}" >/dev/null

# Sourcing the rendered library must replace an inherited hostile function.
# Verification and the subsequent bare command then resolve to the same held
# executable bytes; the hostile function must never receive control.
# shellcheck disable=SC2016
wrapper_command='jankurai() { printf hostile >"$3"; return 97; }; source "$1"; require_jankurai; [[ "$(command -v jankurai)" == jankurai ]]; [[ "$(jankurai --version)" == "$JERYU_JANKURAI_VERSION" ]]; [[ ! -e "$3" ]]'
env -i HOME="${tmp}/home" \
  PATH="${tmp}/home/.local/bin:${tmp}/home/.jeryu/bin:/usr/bin:/bin" \
  bash -c "${wrapper_command}" bash "${test_lib}" "${ambient_bin}" \
  "${tmp}/hostile-function-executed"

ln "${ambient_bin}" "${tmp}/home/.jeryu/bin/jankurai-linked"
expect_failure "linked ordinary auditor library" \
  "governed jankurai custody mismatch: expected one link" \
  env -i HOME="${tmp}/home" \
  PATH="${tmp}/home/.local/bin:${tmp}/home/.jeryu/bin:/usr/bin:/bin" \
  bash -c "${ordinary_command}" bash "${test_lib}" "${ambient_bin}"
expect_failure "linked ordinary auditor verifier" \
  "governed jankurai custody mismatch: expected one link" \
  env -i HOME="${tmp}/home" \
  PATH="${tmp}/home/.local/bin:${tmp}/home/.jeryu/bin:/usr/bin:/bin" \
  bash "${test_verifier}"
rm -f -- "${tmp}/home/.jeryu/bin/jankurai-linked"

run_release_broker() {
  local path="$1"
  local release_command
  shift
  # The child shell expands its positional inputs.
  # shellcheck disable=SC2016
  release_command='source "$1"; require_jankurai; [[ "$JERYU_GOVERNED_JANKURAI_BIN" == "$2" ]]'
  env -i HOME="${tmp}/home" PATH="${path}:/usr/bin:/bin" \
    JAIN_RELEASE_CI=1 "$@" bash -c \
    "${release_command}" \
    bash "${test_lib}" "${broker_bin}"
}

run_release_broker "${tmp}/broker/bin"
env -i HOME="${tmp}/home" PATH="${tmp}/broker/bin:/usr/bin:/bin" \
  JAIN_RELEASE_CI=1 bash "${test_verifier}" >/dev/null

# Caller substitutions never select the auditor: the broker-controlled PATH and
# fixed authority path remain decisive.
run_release_broker "${tmp}/broker/bin" \
  JERYU_GOVERNED_JANKURAI_BIN="${attacker_bin}" \
  JERYU_JANKURAI_BIN="${attacker_bin}"
expect_failure "caller receipt substitution" \
  "release broker Jankurai rejects caller receipt authority" \
  run_release_broker "${tmp}/broker/bin" \
  JERYU_JANKURAI_RECEIPT="${tmp}/caller-receipt.json" \
  JERYU_JANKURAI_RECEIPT_SHA256="$(printf 'a%.0s' {1..64})" \
  JERYU_JANKURAI_ALLOW_TEST_RECEIPT=1

expect_failure "ambient home auditor" "release broker Jankurai path mismatch" \
  run_release_broker "${tmp}/home/.jeryu/bin"
expect_failure "caller PATH substitution" "release broker Jankurai path mismatch" \
  run_release_broker "${tmp}/attacker/bin"
expect_failure "missing broker auditor" "release broker Jankurai path mismatch" \
  run_release_broker "/usr/bin:/bin"

cp -- "${broker_bin}" "${tmp}/governed-backup"
chmod 0755 "${broker_bin}"
printf '#!/usr/bin/env bash\nprintf "jankurai 1.6.11\\n"\n' >"${broker_bin}"
chmod 0555 "${broker_bin}"
expect_failure "wrong broker binary" "governed jankurai identity mismatch" \
  run_release_broker "${tmp}/broker/bin"
rm -f -- "${broker_bin}"
mv -- "${tmp}/governed-backup" "${broker_bin}"
chmod 0555 "${broker_bin}"

chmod 0755 "${broker_bin}"
expect_failure "writable broker binary" "release broker Jankurai custody mismatch" \
  run_release_broker "${tmp}/broker/bin"
chmod 0555 "${broker_bin}"

ln "${broker_bin}" "${tmp}/broker/bin/jankurai-linked"
expect_failure "linked broker binary" "release broker Jankurai custody mismatch" \
  run_release_broker "${tmp}/broker/bin"
rm -f -- "${tmp}/broker/bin/jankurai-linked"

printf 'governed Jankurai path tests passed: broker ambient env path missing identity custody\n'
