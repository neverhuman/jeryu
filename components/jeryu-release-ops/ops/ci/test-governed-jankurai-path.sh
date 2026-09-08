#!/usr/bin/env bash
# Fail-closed selection tests for the release broker's governed Jankurai path.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../.." && pwd)"
source_lib="${here}/lib.sh"
source_verifier="${here}/ensure-jankurai.sh"
production_broker="/opt/jain-ci/authority/release-bin/jankurai"
tmp="$(mktemp -d /tmp/test-governed-jankurai-path.XXXXXX)"
cleanup() {
  rm -rf -- "${tmp}"
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

for source in \
  "${source_lib}" \
  "${source_verifier}"; do
  grep -Fq "${production_broker}" "${source}" ||
    fail "release broker path contract is absent from ${source#"${repo_root}/"}"
  grep -Fq 'local mode=receipt-bound' "${source}" ||
    fail "release broker mode is absent from ${source#"${repo_root}/"}"
  grep -Fq 'local expected_governed="/home/ubuntu/.jeryu/bin/jankurai"' \
    "${source}" ||
    fail "ordinary governed-home selection is absent from ${source#"${repo_root}/"}"
done
for entrypoint in \
  "${here}/pr-ci.sh" \
  "${repo_root}/scripts/ci-doctor.sh"; do
  if grep -Eq '/home/ubuntu/\.jeryu/bin/jankurai|JERYU_JANKURAI_BIN:-' \
    "${entrypoint}"; then
    fail "${entrypoint#"${repo_root}/"} still selects an ambient-home or caller-provided auditor"
  fi
done

mkdir -p "${tmp}/broker/bin" "${tmp}/attacker/bin" \
  "${tmp}/home/.jeryu/bin" "${tmp}/home/.jeryu/receipts/jankurai/sha256" \
  "${tmp}/home/.local/bin"
governed_source="/usr/local/libexec/jain/jankurai"
if [[ ! -x "${governed_source}" ]]; then
  governed_source="$(command -v jankurai 2>/dev/null || true)"
fi
[[ "${governed_source}" == /* && -f "${governed_source}" &&
   ! -L "${governed_source}" && -x "${governed_source}" ]] ||
  fail "governed Jankurai test source is unavailable"
[[ "$("${governed_source}" --version)" == 'jankurai 1.6.11' ]] ||
  fail "governed Jankurai test source has the wrong version"
[[ "$(sha256sum "${governed_source}" | awk '{print $1}')" == \
   '9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c' ]] ||
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
chmod 0755 "${older_local_bin}"

receipt_stage="${tmp}/local-receipt.json"
jq -n \
  --arg path "${ambient_bin}" \
  '{
    schema: "jeryu.jankurai-installation/v2",
    source: {
      remote: "http://127.0.0.1:8787/git/jeryu/jankurai.git",
      commit: "b88562fdb124aa86dedd70ab972e7d0d87e58be1",
      tag: "v1.6.11-deadlang-precision-split.3",
      tree: "611229e54938c0e8808896e369fd54d095d258f7",
      archive_sha256: "903a231eca8f6a1f050953b603d5a278a1606abcdf47434eb1b45262d74068aa",
      cargo_lock_sha256: "b9acb981c326226a687d0b6703e4f7ee303148e9e1a6dda1aa03d77988820f6a",
      verification: "release-authoritative"
    },
    build: {
      rustc: "rustc 1.95.0 (59807616e 2026-04-14)",
      cargo: "cargo 1.95.0 (f2d3ce0bd 2026-03-21)",
      target_triple: "x86_64-unknown-linux-gnu",
      mode: "oci-vendor-locked-offline-workspace-member-v2",
      package_path: "crates/jankurai",
      builder_image: "rust@sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082",
      builder_image_id: "sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082",
      linker: "GNU ld (GNU Binutils for Debian) 2.40",
      glibc: "ldd (Debian GLIBC 2.36-9+deb12u14) 2.36",
      vendor_files_sha256: "a7e332f4495d9748ea020ae8ee37c4240f0f035059799bd3dc74497437143d99",
      vendor_file_count: "14889",
      cargo_config_sha256: "b8982c761d62e447f2d1653c199d2d58e6b2de6c5a6f8ddba3d38e47b7f863d6",
      environment: "CARGO_NET_OFFLINE=true,HOME=/tmp,LANG=C,LC_ALL=C,SOURCE_DATE_EPOCH=0,TZ=UTC",
      rustflags: "--remap-path-prefix=/opt/jeryu/jankurai=/jankurai-build/source --remap-path-prefix=/opt/jeryu/vendor=/jankurai-build/vendor --remap-path-prefix=/opt/jeryu/target=/jankurai-build/target --remap-path-prefix=/usr/local/cargo=/jankurai-build/cargo",
      command: "cargo install --locked --offline --path /opt/jeryu/jankurai/crates/jankurai --root /opt/jeryu/out --bin jankurai",
      context_sha256: "889d19f86fc390b0f0cf0bd6ecb4d451c51a2d6fb328e5520e4310e7ee5dedd6",
      cargo_net_offline: true,
      closed_vendor: true,
      network_none: true,
      read_only_root: true,
      non_root: true,
      capabilities_dropped: true,
      no_new_privileges: true,
      container_engine_path: "/usr/bin/docker",
      git_global_config_disabled: true,
      git_system_config_disabled: true,
      git_http_follow_redirects: false,
      git_terminal_prompt: false,
      jankurai_update_check: false,
      network_scope: "local-forge-source-plus-closed-vendor-network-none",
      no_proxy: "127.0.0.1,localhost,::1"
    },
    governance: {
      status: "governed",
      manifest_repo: "http://127.0.0.1:8787/git/jeryu/jeryu-tool.git",
      manifest_commit: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      manifest_tree: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      manifest_sha256: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      protected_main: true,
      protection_policy: "immutable-main-v1"
    },
    binary: {
      sha256: "9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c",
      version_output: "jankurai 1.6.11"
    },
    installation: {path: $path, atomic: true},
    test_mode: false,
    conclusion: "success"
  }' >"${receipt_stage}"
receipt_sha="$(sha256sum "${receipt_stage}")"
receipt_sha="${receipt_sha%% *}"
local_receipt="${tmp}/home/.jeryu/receipts/jankurai/sha256/${receipt_sha}.json"
mv -- "${receipt_stage}" "${local_receipt}"
caller_receipt_stage="${tmp}/caller-local-receipt.json"
jq --arg path "${attacker_bin}" '.installation.path = $path' \
  "${local_receipt}" >"${caller_receipt_stage}"
caller_receipt_sha="$(sha256sum "${caller_receipt_stage}")"
caller_receipt_sha="${caller_receipt_sha%% *}"
caller_receipt="${tmp}/${caller_receipt_sha}.json"
mv -- "${caller_receipt_stage}" "${caller_receipt}"

# Exercise the exact production logic without requiring write access beneath
# /opt: only this automatically removed test copy substitutes the fixed broker
# path, while every other byte remains the reviewed source.
test_lib="${tmp}/lib.sh"
sed "s#${production_broker}#${broker_bin}#g" "${source_lib}" >"${test_lib}"
test_verifier="${tmp}/ensure-jankurai.sh"
sed "s#${production_broker}#${broker_bin}#g" "${source_verifier}" >"${test_verifier}"
test_local_lib="${tmp}/local-lib.sh"
sed -e "s#${production_broker}#${broker_bin}#g" \
  -e "s#/home/ubuntu/.jeryu#${tmp}/home/.jeryu#g" \
  "${source_lib}" >"${test_local_lib}"

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

# Ordinary local mode keeps the governed home installation authoritative even
# when an older same-version binary appears earlier on the real default PATH.
# shellcheck disable=SC2016
env -i HOME="${tmp}/home" PATH="${tmp}/home/.local/bin:/usr/bin:/bin" \
  bash -c 'source "$1"; require_jankurai; [[ "$JERYU_GOVERNED_JANKURAI_BIN" == "$2" ]]; [[ "$(command -v jankurai)" == "$2" ]]' \
  bash "${test_local_lib}" "${ambient_bin}"
# shellcheck disable=SC2016
env -i HOME="${tmp}/home" PATH="/usr/bin:/bin" \
  JERYU_GOVERNED_JANKURAI_BIN="${attacker_bin}" \
  JERYU_JANKURAI_RECEIPT="${caller_receipt}" \
  bash -c 'source "$1"; require_jankurai; [[ "$JERYU_GOVERNED_JANKURAI_BIN" == "$2" ]]' \
  bash "${test_local_lib}" "${attacker_bin}"

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
chmod 0755 "${broker_bin}"
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

printf 'governed Jankurai path tests passed: broker ambient env path receipt missing identity custody\n'
