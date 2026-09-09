#!/usr/bin/env bash
# Contract fixtures only: no auditor, scanner, compiler, installer or real evidence runs.
set -euo pipefail
umask 077
# Environment bindings consumed by the sourced producer functions and JSON fragments.
export expected_auditor_receipt_sha JERYU_MONOREPO_CANDIDATE binary_sha binary_size \
  release_tag package_version binary_version_output split_revision \
  cargo_toml_sha cargo_lock_sha rust_toolchain_sha version_file_sha \
  cli_help_sha cargo_bin rustc_bin cargo_sha \
  rustc_sha cargo_version rustc_version cargo_home_config_sha \
  git_global_config_sha command score_evidence_sha security_evidence_sha \
  artifact_schema artifact_status build_authority_mode input_prefix \
  workspace_manifest_sha candidate_cargo_configuration private_target_identity artifact_tmp_identity \
  score_report_predicate
component_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
producer="$component_root/ops/ci/artifact-support.sh"
score_report_predicate="$component_root/ops/ci/score-report.jq"
# shellcheck source=/dev/null
source "$component_root/tests/scratch.sh"
# shellcheck source=/dev/null
source "$component_root/ops/ci/candidate-artifact.sh"
# Load only the real pure validators and cleanup functions, without the producer entrypoint.
# shellcheck source=/dev/null
source <(sed -n \
  -e '/^die() {$/,/^}$/p' \
  -e '/^sha_file() {$/,/^}$/p' \
  -e '/^require_dir() {$/,/^}$/p' \
  -e '/^require_file() {$/,/^}$/p' \
  -e '/^require_physical_file() {$/,/^}$/p' \
  -e '/^require_physical_dir() {$/,/^}$/p' \
  -e '/^validate_score() {$/,/^}$/p' \
  -e '/^verify_score_evidence() {$/,/^}$/p' \
  -e '/^validate_security() {$/,/^}$/p' \
  -e '/^verify_security_evidence() {$/,/^}$/p' \
  -e '/^receipt_matches() {$/,/^}$/p' \
  -e '/^cleanup() {$/,/^}$/p' "$producer")
# shellcheck source=/dev/null
source <(sed -n -e '/^jeryu_source_fail() {$/,/^}$/p' \
  -e '/^jeryu_require_score_report_matches_source() {$/,/^}$/p' \
  "$component_root/ops/ci/source-authority.sh")
scratch="$(mktemp -d /tmp/jeryu-finder-artifact-contract.XXXXXX)"
jeryu_record_test_scratch "$scratch"
test_cleanup() {
  local status=$?
  jeryu_remove_test_scratch || status=1
  exit "$status"
}
trap test_cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
fail() { printf 'candidate artifact contract failed: %s\n' "$*" >&2; exit 1; }
reject() {
  if ("$@") >"$scratch/out" 2>"$scratch/error"; then fail "accepted: $1"; fi
}

candidate_root="$scratch/monorepo"
repo_root="$candidate_root/components/jeryu-tool-finder"
cargo_home="$scratch/cargo-home"
mkdir -p "$repo_root/agent" "$repo_root/target/jankurai" "$repo_root/target/security" "$cargo_home"
cd "$repo_root"
config="$cargo_home/config.toml"
jeryu_candidate_cargo_config_record "$config" | jq -e '.state == "absent"' >/dev/null
empty_configuration="$(jeryu_candidate_cargo_config_snapshot)"
: >"$config"
jeryu_candidate_cargo_config_record "$config" | jq -e '.state == "file" and (.sha256|length) == 64' >/dev/null
[[ "$(jeryu_candidate_cargo_config_snapshot)" != "$empty_configuration" ]] || fail 'config appearance omitted'
printf '[build]\njobs = 2\n\n[net]\ngit-fetch-with-cli = true\n' >"$config"
jeryu_candidate_cargo_config_record "$config" >/dev/null
printf '# allowed comment\n\n[build] # section\njobs = 1 # value\n[net]\ngit-fetch-with-cli = true\n' >"$config"
jeryu_candidate_cargo_config_record "$config" >/dev/null
for content in \
  'include = ["outside.toml"]' \
  '[env]\nRUSTFLAGS = { value = "changed", force = true }' \
  '[build]\nrustc-wrapper = "/other"' \
  '[build]\njobs = true' \
  '[build]\njobs = 3' \
  '[net]\ngit-fetch-with-cli = false' \
  '[source.crates-io]\nreplace-with = "other"' \
  '[profile.release]\nopt-level = 0' \
  '[build]\njobs = 2\njobs = 2' \
  '[build]\n[build]' \
  '[net]\ngit-fetch-with-cli = true\ngit-fetch-with-cli = true' \
  '[net]\n[net]' \
  'jobs = 2' \
  '[build]\njobs = 02' \
  '[build]\njobs = +2' \
  '[build]\njobs = """2"""' \
  '[build'; do
  printf '%b\n' "$content" >"$config"
  reject jeryu_candidate_cargo_config_record "$config"
done
rm -- "$config"
ln -s "$scratch/absent" "$config"
reject jeryu_candidate_cargo_config_record "$config"
rm -- "$config"
printf '[net]\ngit-fetch-with-cli = true\n' >"$cargo_home/config"
jeryu_candidate_cargo_config_snapshot | jq -e --arg path "$cargo_home/config" \
  'any(.[]; .path == $path and .state == "file")' >/dev/null
rm -- "$cargo_home/config"
mkdir "$candidate_root/.cargo"
printf 'include = ["outside.toml"]\n' >"$candidate_root/.cargo/config"
reject jeryu_candidate_cargo_config_snapshot
rm -- "$candidate_root/.cargo/config"
rmdir "$candidate_root/.cargo"
ln -s "$cargo_home" "$candidate_root/.cargo"
reject jeryu_candidate_cargo_config_snapshot
rm -- "$candidate_root/.cargo"
: >"$cargo_home/credentials.toml"
reject jeryu_candidate_cargo_config_snapshot
rm -- "$cargo_home/credentials.toml"

current_head="$(printf '%040d' 1)"
current_tree="$(printf '%040d' 2)"
current_source_sha="$(printf '%064d' 3)"
digest="$(printf '%064d' 4)"
scope="$(jq -cn --arg head "$current_head" --arg tree "$current_tree" --arg sha "$current_source_sha" '
  {schema:"jeryu.monorepo-candidate.source/v1",
   monorepo:{repository:"https://github.com/neverhuman/jeryu.git",commit:$head,tree:$tree,tracked_inputs_sha256:$sha},
   component:{path:"components/jeryu-tool-finder",tree:$tree,tracked_inputs_sha256:$sha},
   governance:{protected_main:false,handover:"pending"}}')"
printf 'minimum_score = 85\n' >agent/audit-policy.toml
policy_sha="$(sha_file agent/audit-policy.toml)"
jq -n --arg head "$current_head" --arg fingerprint "sha256:$digest" '
  {repo:".",git:{head:$head,dirty_worktree:false},score:90,raw_score:90,
   caps_applied:[],findings:[],decision:{hard_findings:0,passed:true,status:"pass",minimum_score:85},
   scope:{mode:"full",paths:[]},policy:{mode:"standard",minimum_score:85,fail_on:["critical","high"]},
   copy_code:{status:"pass",classes:[],summary:{hard_classes:0,hard_instances:0}},
   report_fingerprint:$fingerprint,input_fingerprint:$fingerprint,policy_fingerprint:$fingerprint}
' >target/jankurai/repo-score.json
report_sha="$(sha_file target/jankurai/repo-score.json)"
printf 'unit identity bytes; not an installation receipt\n' >"$scratch/identity.fixture"
expected_auditor_path="$scratch/identity.fixture"
expected_auditor_sha="$(sha_file "$expected_auditor_path")"
expected_auditor_version='fixture identity'
expected_auditor_receipt="$scratch/identity.fixture"
expected_auditor_receipt_sha="$expected_auditor_sha"
printf '{}\n' >target/security/cargo-audit.json
printf '{"spdxVersion":"SPDX-2.3","SPDXID":"SPDXRef-FIXTURE","packages":[]}\n' \
  >target/security/jeryu-tool-finder.spdx.json
audit_sha="$(sha_file target/security/cargo-audit.json)"
sbom_sha="$(sha_file target/security/jeryu-tool-finder.spdx.json)"

# The same actual consumers retain legacy shape and require candidate scope in explicit mode.
for mode in 0 1; do
  JERYU_MONOREPO_CANDIDATE=$mode
  score_schema='jeryu.split.score/v1'
  security_schema='jeryu.split.security/v1'
  expected_auditor_mode='installation-receipt'
  source_scope=null
  if [[ $mode == 1 ]]; then
    score_schema='jeryu.monorepo-candidate.score/v1'
    security_schema='jeryu.monorepo-candidate.security/v1'
    expected_auditor_mode='public-candidate-installation'
    source_scope=$scope
  fi
  current_source_json="$(jq -cn --arg sha "$current_source_sha" --argjson scope "$source_scope" \
    '{tracked_inputs_sha256:$sha} + (if $scope == null then {} else {scope:$scope} end)')"
  jq -n --arg schema "$score_schema" --arg head "$current_head" --arg tree "$current_tree" \
    --argjson source "$current_source_json" --arg path "$expected_auditor_path" \
    --arg sha "$expected_auditor_sha" --arg version "$expected_auditor_version" \
    --arg mode "$expected_auditor_mode" --arg receipt "$expected_auditor_receipt" \
    --arg report "$report_sha" --arg policy "$policy_sha" --arg fingerprint "sha256:$digest" '
    {schema_version:$schema,repo:"jeryu-tool-finder",status:"pass",head:$head,tree:$tree,source:$source,
     auditor:{path:$path,sha256:$sha,version_output:$version,authority_mode:$mode,
       installation_receipt:{path:$receipt,sha256:$sha}},
     policy:{path:"agent/audit-policy.toml",sha256:$policy},
     report:{path:"target/jankurai/repo-score.json",sha256:$report,score:90,raw_score:90,
       minimum_score:85,caps_applied:[],hard_findings:0,
       report_fingerprint:$fingerprint,input_fingerprint:$fingerprint,policy_fingerprint:$fingerprint}}
  ' >target/jankurai/evidence.json
  jq -n --arg schema "$security_schema" --arg head "$current_head" --arg tree "$current_tree" \
    --argjson source "$current_source_json" --arg audit "$audit_sha" --arg sbom "$sbom_sha" '
    {schema_version:$schema,repo:"jeryu-tool-finder",status:"pass",head:$head,tree:$tree,source:$source,
     checks:["gitleaks-detect","actionlint","env-file","cargo-metadata","cargo-deny-locked-policy",
       "cargo-audit-no-fetch","syft-sbom"],cargo_audit:"clean",sbom:"generated",
     artifacts:{cargo_audit:{path:"target/security/cargo-audit.json",sha256:$audit},
       sbom:{path:"target/security/jeryu-tool-finder.spdx.json",sha256:$sbom}}}
  ' >target/security/evidence.json
  validate_score "$current_head" "$current_tree" "$current_source_sha"
  validate_security "$current_head" "$current_tree" "$current_source_sha"
  for kind in score security; do
    if [[ $kind == score ]]; then evidence=target/jankurai/evidence.json; else evidence=target/security/evidence.json; fi
    original="$(cat "$evidence")"
    for mutation in \
      '.schema_version = "foreign"' \
      '.head = ("0" * 40)' \
      '.tree = ("0" * 40)' \
      '.source.tracked_inputs_sha256 = ("0" * 64)' \
      '.source.scope = {}'; do
      jq "$mutation" <<<"$original" >"$evidence"
      reject "validate_$kind" "$current_head" "$current_tree" "$current_source_sha"
    done
    if [[ $mode == 1 ]]; then
      for mutation in 'del(.source.scope)' '.source.scope.governance.protected_main = true' \
        '.source.scope.component.path = "components/other"' '.source.scope.monorepo.commit = ("0" * 40)'; do
        jq "$mutation" <<<"$original" >"$evidence"
        reject "validate_$kind" "$current_head" "$current_tree" "$current_source_sha"
      done
    fi
    printf '%s\n' "$original" >"$evidence"
  done
done

# Rebinding the report digest cannot turn a foreign Git report into current-source proof.
report_original="$(cat target/jankurai/repo-score.json)"
score_original="$(cat target/jankurai/evidence.json)"
for mutation in '.findings = [{severity:"high"}]' '.caps = ["concealed cap"]' \
  '.decision.status = "advisory"'; do
  jq "$mutation" <<<"$report_original" >target/jankurai/repo-score.json
  changed_sha="$(sha_file target/jankurai/repo-score.json)"
  jq --arg sha "$changed_sha" '.report.sha256 = $sha' <<<"$score_original" >target/jankurai/evidence.json
  reject validate_score "$current_head" "$current_tree" "$current_source_sha"
done
jq '.git.head = ("0" * 40)' <<<"$report_original" >target/jankurai/repo-score.json
changed_sha="$(sha_file target/jankurai/repo-score.json)"
jq --arg sha "$changed_sha" '.report.sha256 = $sha' <<<"$score_original" >target/jankurai/evidence.json
reject validate_score "$current_head" "$current_tree" "$current_source_sha"
printf '%s\n' "$report_original" >target/jankurai/repo-score.json
printf '%s\n' "$score_original" >target/jankurai/evidence.json

# Exercise the real artifact JSON constructor and consumer with private fixture bytes.
binary_name='jeryu-tool-finder'
artifact_tmp="$scratch/binary.fixture"
receipt_tmp="$scratch/receipt.fixture"
printf 'unit binary bytes; never executed\n' >"$artifact_tmp"
binary_sha="$(sha_file "$artifact_tmp")"
binary_size="$(stat -c '%s' "$artifact_tmp")"
release_tag='jeryu-tool-finder-v5.1.0-split.0'
package_version='5.1.0'
binary_version_output='jeryu-tool-finder 5.1.0'
split_revision=0
cargo_toml_sha=$digest
cargo_lock_sha=$digest
rust_toolchain_sha=$digest
version_file_sha=$digest
cli_help_sha=$digest
cargo_bin=/fixture/cargo
rustc_bin=/fixture/rustc
cargo_sha=$digest
rustc_sha=$digest
cargo_version='cargo fixture'
rustc_version='rustc fixture'
cargo_home_config_sha=$digest
git_global_config_sha=$digest
build_jobs=2
command="cargo build --locked --offline --release --bin $binary_name --jobs $build_jobs"
score_evidence_sha="$(sha_file target/jankurai/evidence.json)"
security_evidence_sha="$(sha_file target/security/evidence.json)"
for mode in 0 1; do
  artifact_schema='jeryu.split.artifact-support/v2'
  artifact_status=ready
  build_authority_mode='local-governed-toolchain'
  input_prefix=''
  workspace_manifest_sha=''
  source_scope=null
  candidate_cargo_configuration=null
  if [[ $mode == 1 ]]; then
    artifact_schema='jeryu.monorepo-candidate.artifact-support/v1'
    artifact_status=candidate-ready
    build_authority_mode='monorepo-candidate-toolchain'
    input_prefix='components/jeryu-tool-finder/'
    workspace_manifest_sha=$digest
    source_scope=$scope
    candidate_cargo_configuration="$(jeryu_candidate_cargo_config_snapshot)"
  fi
  current_source_json="$(jq -cn --arg sha "$current_source_sha" --argjson scope "$source_scope" \
    '{tracked_inputs_sha256:$sha} + (if $scope == null then {} else {scope:$scope} end)')"
  # These exact fragments only bind JSON; no release-input or build entrypoint runs.
  # shellcheck source=/dev/null
  source <(sed -n '/^  receipt_inputs_json=/,/^}$/p' "$producer" | sed '$d')
  # shellcheck source=/dev/null
  source <(sed -n '/^  jq -nS /,/^  chmod 0600 "\$receipt_tmp"/p' "$producer")
  receipt_matches "$receipt_tmp" "$artifact_tmp" 2 || fail 'artifact JSON constructor/consumer mismatch'
  if [[ $mode == 1 ]]; then
    jq -e '.schema_version == "jeryu.monorepo-candidate.artifact-support/v1" and
      .status == "candidate-ready" and .source.scope.governance == {protected_main:false,handover:"pending"} and
      .inputs.cargo_manifest.path == "components/jeryu-tool-finder/Cargo.toml" and
      .inputs.workspace_manifest.path == "Cargo.toml" and .inputs.cargo_lock.path == "Cargo.lock" and
      .inputs.rust_toolchain.path == "rust-toolchain.toml" and
      (.build | has("cargo_configuration")) and (.build | has("cargo_home_config_sha256") | not)
    ' "$receipt_tmp" >/dev/null
  else
    jq -e '.schema_version == "jeryu.split.artifact-support/v2" and .status == "ready" and
      (.source|keys) == ["tracked_inputs_sha256"] and (.inputs|has("workspace_manifest")|not) and
      (.build|has("cargo_configuration")|not)
    ' "$receipt_tmp" >/dev/null
  fi
  original="$(cat "$receipt_tmp")"
  for mutation in \
    '.schema_version = "foreign"' \
    '.head = ("0" * 40)' \
    '.source.tracked_inputs_sha256 = ("0" * 64)' \
    '.inputs.cargo_lock.path = "component/Cargo.lock"' \
    '.evidence.score.sha256 = ("0" * 64)' \
    '.evidence.security.sha256 = ("0" * 64)' \
    '.build.cargo.sha256 = ("0" * 64)' \
    '.build.jobs = 0'; do
    jq "$mutation" <<<"$original" >"$receipt_tmp"
    reject receipt_matches "$receipt_tmp" "$artifact_tmp"
  done
  if [[ $mode == 1 ]]; then
    for mutation in 'del(.source.scope)' '.source.scope.governance.protected_main = true' \
      'del(.inputs.workspace_manifest)' '.build.cargo_configuration = []' \
      '.build.jobs = 3 | .build.command = "cargo build --locked --offline --release --bin jeryu-tool-finder --jobs 3"'; do
      jq "$mutation" <<<"$original" >"$receipt_tmp"
      reject receipt_matches "$receipt_tmp" "$artifact_tmp"
    done
  fi
done

# Production cleanup uses separately recorded roots and refuses an escaping link.
private_target="$scratch/private-target"
artifact_tmp_dir="$scratch/pair"
mkdir "$private_target" "$artifact_tmp_dir"
private_target_identity="$(jeryu_record_test_scratch "$private_target"; printf '%s\n' "${jeryu_test_scratch_identity:?}")"
artifact_tmp_identity="$(jeryu_record_test_scratch "$artifact_tmp_dir"; printf '%s\n' "${jeryu_test_scratch_identity:?}")"
printf 'retained\n' >"$scratch/sentinel"
ln -s "$scratch/sentinel" "$private_target/external"
if cleanup 0; then fail 'producer cleanup accepted an external link'; fi
[[ -d $private_target && -z $artifact_tmp_dir && "$(<"$scratch/sentinel")" == retained ]] ||
  fail 'producer cleanup refusal changed custody'
rm -- "$private_target/external"
cleanup 0
[[ -z $private_target ]] || fail 'producer target was retained after safe cleanup'
printf 'candidate artifact config/provenance/cleanup contract tests ok\n'
