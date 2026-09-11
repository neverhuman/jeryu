#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$repo_root"
source "$repo_root/ops/ci/lib.sh"
source "$repo_root/ops/ci/source-authority.sh"
source "$repo_root/tests/scratch.sh"
source "$repo_root/ops/ci/candidate-artifact.sh"
source "$repo_root/ops/ci/candidate-dependencies.sh"
score_schema='jeryu.split.score/v1'
security_schema='jeryu.split.security/v1'
artifact_schema='jeryu.split.artifact-support/v2'
artifact_status='ready'
score_report_predicate="$repo_root/ops/ci/score-report.jq"
candidate_cargo_configuration=null
if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
  score_schema='jeryu.monorepo-candidate.score/v1'
  security_schema='jeryu.monorepo-candidate.security/v1'
  artifact_schema='jeryu.monorepo-candidate.artifact-support/v1'
  artifact_status='candidate-ready'
fi

receipt_rel='target/artifact-support/jeryu-tool-finder.json'
receipt="$repo_root/$receipt_rel"
artifact_rel='target/artifact-support/jeryu-tool-finder'
artifact="$repo_root/$artifact_rel"
binary_name='jeryu-tool-finder'
private_target=''
private_target_identity=''
artifact_tmp_identity=''
private_target_parent=''
artifact_tmp=''
artifact_tmp_dir=''
receipt_tmp=''
help_stdout=''
help_stderr=''
version_stdout=''
version_stderr=''

die() {
  printf 'artifact-support failed: %s\n' "$*" >&2
  exit 1
}

sha_file() {
  sha256sum <"$1" | awk '{print $1}'
}

cleanup() {
  local status=${1:-0}
  if [[ -n ${artifact_tmp_dir:-} ]]; then
    if jeryu_test_scratch="$artifact_tmp_dir" \
      jeryu_test_scratch_identity="$artifact_tmp_identity" jeryu_remove_test_scratch; then
      artifact_tmp_dir=''
    else
      status=1
    fi
  fi
  if [[ -n ${private_target:-} ]]; then
    if jeryu_test_scratch="$private_target" \
      jeryu_test_scratch_identity="$private_target_identity" jeryu_remove_test_scratch; then
      private_target=''
    else
      status=1
    fi
  fi
  return "$status"
}
trap 'status=$?; cleanup "$status" || status=$?; exit "$status"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

require_dir() {
  local relative="$1"
  local path="$repo_root/$relative"
  [[ -d "$path" && ! -L "$path" ]] || die "$relative is not a physical directory"
  [[ "$(realpath -e -- "$path")" == "$path" ]] || die "$relative escapes the repository"
}

require_file() {
  local relative="$1"
  require_physical_file "$repo_root/$relative" "$relative"
}

require_physical_file() {
  local path="$1"
  local label="${2:-$1}"
  [[ "$path" == /* && -f "$path" && ! -L "$path" ]] ||
    die "$label is not a physical regular file"
  [[ "$(realpath -e -- "$path")" == "$path" ]] || die "$label contains a path alias"
  [[ "$(stat -c '%h' -- "$path")" -eq 1 ]] || die "$label is multiply linked"
}

require_physical_dir() {
  local path="$1"
  local label="${2:-$1}"
  [[ "$path" == /* && -d "$path" && ! -L "$path" ]] ||
    die "$label is not a physical directory"
  [[ "$(realpath -e -- "$path")" == "$path" ]] || die "$label contains a path alias"
}

prepare_output_dirs() {
  if [[ ! -e target && ! -L target ]]; then
    mkdir -- target
  fi
  require_dir target
  if [[ ! -e target/artifact-support && ! -L target/artifact-support ]]; then
    mkdir -- target/artifact-support
  fi
  require_dir target/artifact-support
  if [[ -e "$artifact" || -L "$artifact" ]]; then
    require_file "$artifact_rel"
  fi
  if [[ -e "$receipt" || -L "$receipt" ]]; then
    require_file "$receipt_rel"
  fi
}

validate_build_environment() {
  local record name local_home expected_cargo_launcher expected_rustc_launcher
  local actual_cargo_launcher actual_rustc_launcher expected_release_target
  local actual_cargo_real actual_rustc_real expected_cargo_real expected_rustc_real
  local release_tmp cargo_config_body

  while IFS= read -r -d '' record; do
    name="${record%%=*}"
    case "$name" in
      CARGO|RUSTC|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|RUSTDOC|RUSTFLAGS|\
      RUSTDOCFLAGS|RUSTUP_TOOLCHAIN|CARGO_ENCODED_RUSTFLAGS|CARGO_INCREMENTAL|\
      CARGO_PROFILE_*|CARGO_BUILD_TARGET|CARGO_BUILD_RUSTC|\
      CARGO_BUILD_RUSTC_WRAPPER|CARGO_BUILD_RUSTDOC|CARGO_BUILD_RUSTFLAGS|\
      CARGO_TARGET_*_LINKER|CC|CXX|AR|LD|CFLAGS|CXXFLAGS|LDFLAGS|\
      CC_*|CXX_*|AR_*|LD_*|TARGET_*_LINKER)
        die "caller build override is forbidden: $name"
        ;;
    esac
  done < <(/usr/bin/env -0)

  [[ "${CARGO_NET_OFFLINE:-true}" == true ]] ||
    die 'CARGO_NET_OFFLINE must be true when supplied'
  build_jobs="${JERYU_CI_JOBS:-${CARGO_BUILD_JOBS:-2}}"
  [[ "$build_jobs" =~ ^[1-9][0-9]*$ && "$build_jobs" -le 256 ]] ||
    die 'governed build job count must be an integer from 1 through 256'
  if [[ -n "${JERYU_CI_JOBS:-}" && -n "${CARGO_BUILD_JOBS:-}" ]]; then
    [[ "$JERYU_CI_JOBS" == "$CARGO_BUILD_JOBS" ]] ||
      die 'JERYU_CI_JOBS and CARGO_BUILD_JOBS disagree'
  fi

  if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
    jeryu_candidate_artifact_build_environment
    return
  fi

  toolchain_channel="$(awk -F'"' '/^[[:space:]]*channel[[:space:]]*=/ {print $2; exit}' \
    rust-toolchain.toml)"
  [[ "$toolchain_channel" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
    die 'rust-toolchain.toml does not contain an exact stable channel'

  if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
    build_authority_mode='release-broker'
    expected_cargo_launcher='/opt/jain-ci/cargo-bin/cargo'
    expected_rustc_launcher='/opt/jain-ci/cargo-bin/rustc'
    cargo_bin="/opt/jain-ci/rustup/toolchains/${toolchain_channel}-x86_64-unknown-linux-gnu/bin/cargo"
    rustc_bin="/opt/jain-ci/rustup/toolchains/${toolchain_channel}-x86_64-unknown-linux-gnu/bin/rustc"
    [[ "$(command -v cargo 2>/dev/null || true)" == "$expected_cargo_launcher" &&
       "$(command -v rustc 2>/dev/null || true)" == "$expected_rustc_launcher" ]] ||
      die 'release PATH cargo or rustc differs from the sealed broker'
    cargo_home="${JAIN_HOST_CI_WRITABLE_ROOT:?}/cargo-home"
    rustup_home='/opt/jain-ci/rustup'
    [[ "${CARGO_HOME:-}" == "$cargo_home" && "${RUSTUP_HOME:-}" == "$rustup_home" ]] ||
      die 'release Cargo or Rustup home differs from sealed authority'
    expected_release_target="$(dirname "$repo_root")/cargo-target"
    [[ "${CARGO_TARGET_DIR:-}" == "$expected_release_target" ]] ||
      die 'release CARGO_TARGET_DIR differs from the checkout-private target'
    release_tmp="$(dirname "$repo_root")"
    [[ "$release_tmp" == "$JAIN_HOST_CI_WRITABLE_ROOT"/physical-checkouts/split-host-ci.?????? ]] ||
      die 'release checkout is outside the sealed physical-checkout root'
    private_target_parent="$CARGO_TARGET_DIR"
    git_global_config="$JAIN_HOST_CI_WRITABLE_ROOT/ci-gitconfig"
    [[ "${GIT_CONFIG_GLOBAL:-}" == "$git_global_config" ]] ||
      die 'release Git source configuration differs from sealed authority'
  else
    [[ "${JAIN_RELEASE_CI:-0}" == 0 ]] || die 'JAIN_RELEASE_CI must be 0 or 1'
    build_authority_mode='local-governed-toolchain'
    [[ -z "${CARGO_TARGET_DIR:-}" ]] ||
      die 'local CARGO_TARGET_DIR overrides are forbidden'
    local_home="$(getent passwd "$(id -u)" | awk -F: 'NR == 1 {print $6}')"
    [[ "$local_home" == /* && -d "$local_home" && ! -L "$local_home" &&
       "$(realpath -e -- "$local_home")" == "$local_home" ]] ||
      die 'cannot resolve the physical account home'
    cargo_home="$local_home/.cargo"
    rustup_home="$local_home/.rustup"
    expected_cargo_launcher="$cargo_home/bin/cargo"
    expected_rustc_launcher="$cargo_home/bin/rustc"
    cargo_bin="$rustup_home/toolchains/${toolchain_channel}-x86_64-unknown-linux-gnu/bin/cargo"
    rustc_bin="$rustup_home/toolchains/${toolchain_channel}-x86_64-unknown-linux-gnu/bin/rustc"
    actual_cargo_launcher="$(command -v cargo 2>/dev/null || true)"
    actual_rustc_launcher="$(command -v rustc 2>/dev/null || true)"
    actual_cargo_real="$(realpath -e -- "$actual_cargo_launcher" 2>/dev/null || true)"
    actual_rustc_real="$(realpath -e -- "$actual_rustc_launcher" 2>/dev/null || true)"
    expected_cargo_real="$(realpath -e -- "$expected_cargo_launcher" 2>/dev/null || true)"
    expected_rustc_real="$(realpath -e -- "$expected_rustc_launcher" 2>/dev/null || true)"
    [[ -n "$actual_cargo_launcher" && -n "$actual_rustc_launcher" &&
       "$actual_cargo_real" == "$expected_cargo_real" &&
       "$actual_rustc_real" == "$expected_rustc_real" ]] ||
      die 'PATH cargo or rustc does not resolve through the governed Rustup launcher'
    private_target_parent="$repo_root/target/artifact-support-build"
    if [[ ! -e "$private_target_parent" && ! -L "$private_target_parent" ]]; then
      mkdir -- "$private_target_parent"
    fi
    git_global_config="$local_home/.gitconfig"
  fi

  require_physical_dir "$cargo_home" 'governed Cargo home'
  require_physical_dir "$rustup_home" 'governed Rustup home'
  require_physical_file "$cargo_bin" 'governed cargo binary'
  require_physical_file "$rustc_bin" 'governed rustc binary'
  require_physical_dir "$private_target_parent" 'governed target parent'
  [[ "$(stat -c '%u' -- "$private_target_parent")" == "$(id -u)" ]] ||
    die 'governed target parent is not owned by the current worker'

  cargo_home_config="$cargo_home/config.toml"
  require_physical_file "$cargo_home_config" 'governed Cargo home configuration'
  if [[ "$build_authority_mode" == release-broker ]]; then
    cargo_config_body="$(<"$cargo_home_config")"
    [[ "$cargo_config_body" == $'[net]\ngit-fetch-with-cli = true' ]] ||
      die 'release Cargo home configuration differs from sealed authority'
  elif awk '
      { sub(/[[:space:]]*#.*/, ""); if ($0 != "") print }
    ' "$cargo_home_config" | grep -Eiq \
      '(^\[profile|^\[target|rustc|rustflags|rustdoc|wrapper|runner|linker|target[[:space:]]*=|incremental)'; then
    die 'local Cargo home configuration contains a build-authority override'
  fi
  require_physical_file "$git_global_config" 'governed Git source configuration'

  private_target="$(mktemp -d "$private_target_parent/.jeryu-tool-finder.XXXXXX")"
  chmod 0700 "$private_target"
  private_target_identity=$(
    jeryu_record_test_scratch "$private_target" || exit 1
    printf '%s\n' "${jeryu_test_scratch_identity:?}"
  )
  require_physical_dir "$private_target" 'fresh private Cargo target'
  [[ "$(stat -c '%u:%a' -- "$private_target")" == "$(id -u):700" &&
     -z "$(find "$private_target" -mindepth 1 -maxdepth 1 -print -quit)" ]] ||
    die 'fresh private Cargo target lacks empty private custody'
  mkdir -m 0700 -- "$private_target/home" "$private_target/tmp"

  cargo_sha="$(sha_file "$cargo_bin")"
  rustc_sha="$(sha_file "$rustc_bin")"
  cargo_home_config_sha="$(sha_file "$cargo_home_config")"
  git_global_config_sha="$(sha_file "$git_global_config")"
  cargo_version="$(run_clean_tool "$cargo_bin" --version)"
  rustc_version="$(run_clean_tool "$rustc_bin" --version)"
  [[ "$cargo_version" == "cargo $toolchain_channel "* &&
     "$rustc_version" == "rustc $toolchain_channel "* ]] ||
    die 'Cargo or rustc version differs from rust-toolchain.toml'
}

run_clean_tool() {
  /usr/bin/env -i HOME=/nonexistent PATH=/usr/bin:/bin LANG=C LC_ALL=C TZ=UTC "$@"
}

run_cargo() {
  /usr/bin/env -i \
    HOME="$private_target/home" USER="$(id -un)" LOGNAME="$(id -un)" \
    PATH="$(dirname "$cargo_bin"):/usr/bin:/bin" LANG=C LC_ALL=C TZ=UTC \
    SOURCE_DATE_EPOCH=0 TMPDIR="$private_target/tmp" \
    CARGO_HOME="$cargo_home" RUSTUP_HOME="$rustup_home" \
    CARGO_TARGET_DIR="$private_target" CARGO_NET_OFFLINE=true \
    CARGO_BUILD_JOBS="$build_jobs" CARGO_INCREMENTAL=0 RUSTC="$rustc_bin" \
    GIT_ATTR_NOSYSTEM=1 GIT_CONFIG_NOSYSTEM=1 \
    GIT_CONFIG_GLOBAL="$git_global_config" GIT_NO_REPLACE_OBJECTS=1 \
    GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 \
    GIT_CONFIG_COUNT=5 \
    GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0="$repo_root" \
    GIT_CONFIG_KEY_1=core.fsmonitor GIT_CONFIG_VALUE_1=false \
    GIT_CONFIG_KEY_2=core.hooksPath GIT_CONFIG_VALUE_2=/dev/null \
    GIT_CONFIG_KEY_3=diff.external GIT_CONFIG_VALUE_3= \
    GIT_CONFIG_KEY_4=core.sshCommand GIT_CONFIG_VALUE_4=/usr/bin/false \
    "$cargo_bin" --config net.offline=true --config "build.jobs=$build_jobs" "$@"
}

verify_build_authority() {
  if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
    jeryu_candidate_verify_artifact_build_authority
    return
  fi
  [[ "$(sha_file "$cargo_bin")" == "$cargo_sha" &&
     "$(sha_file "$rustc_bin")" == "$rustc_sha" &&
     "$(sha_file "$cargo_home_config")" == "$cargo_home_config_sha" &&
     "$(sha_file "$git_global_config")" == "$git_global_config_sha" &&
     "$(run_clean_tool "$cargo_bin" --version)" == "$cargo_version" &&
     "$(run_clean_tool "$rustc_bin" --version)" == "$rustc_version" ]] ||
    die 'governed build authority moved during artifact processing'
}

resolve_auditor_authority() {
  require_jankurai
  expected_auditor_path="$JERYU_GOVERNED_JANKURAI_BIN"
  if [[ -n "${JERYU_JANKURAI_BIN:-}" && "$JERYU_JANKURAI_BIN" != "$expected_auditor_path" ]]; then
    die 'caller JERYU_JANKURAI_BIN differs from the governed auditor'
  fi
  require_physical_file "$expected_auditor_path" 'governed Jankurai binary'
  expected_auditor_sha="$(sha_file "$expected_auditor_path")"
  if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
    expected_auditor_version="$(jeryu_candidate_score_auditor --version)"
  else
    expected_auditor_version="$(run_clean_tool "$expected_auditor_path" --version)"
  fi
  [[ "$expected_auditor_sha" == "$JERYU_JANKURAI_SHA256" &&
     "$expected_auditor_version" == "$JERYU_JANKURAI_VERSION" ]] ||
    die 'governed Jankurai identity differs from the rendered pin'
  if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
    expected_auditor_mode='release-broker'
    expected_auditor_receipt=''
    expected_auditor_receipt_sha=''
  else
    expected_auditor_mode='installation-receipt'
    if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
      expected_auditor_mode='public-candidate-installation'
    fi
    expected_auditor_receipt="${JERYU_JANKURAI_RECEIPT:-}"
    expected_auditor_receipt_sha="${JERYU_JANKURAI_RECEIPT_SHA256:-}"
    require_physical_file "$expected_auditor_receipt" 'governed Jankurai installation receipt'
    [[ "$expected_auditor_receipt_sha" =~ ^[0-9a-f]{64}$ &&
       "$(sha_file "$expected_auditor_receipt")" == "$expected_auditor_receipt_sha" ]] ||
      die 'governed Jankurai installation receipt moved'
  fi
}

validate_score() {
  local head="$1" tree="$2" source_sha="$3"
  local evidence='target/jankurai/evidence.json'
  local report='target/jankurai/repo-score.json'
  local policy='agent/audit-policy.toml'
  local report_sha policy_sha floor

  require_dir target/jankurai
  require_file "$evidence"
  require_file "$report"
  require_file "$policy"
  score_evidence_sha="$(sha_file "$evidence")"
  score_report_sha="$(sha_file "$report")"
  score_policy_sha="$(sha_file "$policy")"
  report_sha="$score_report_sha"
  policy_sha="$score_policy_sha"
  floor="$(awk -F= '/^[[:space:]]*minimum_score[[:space:]]*=/ {
    gsub(/[[:space:]]/, "", $2); print $2; exit
  }' "$policy")"
  [[ "$floor" =~ ^[0-9]+$ && $floor -ge 75 && $floor -le 100 ]] ||
    die 'score policy floor must be an integer from 75 through 100'

  if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
    jeryu_require_score_report_matches_source "$report" "$head" ||
      die 'candidate score report Git identity differs'
  fi
  jq -e --arg schema "$score_schema" --argjson source "$current_source_json" \
    --slurpfile raw "$report" --arg head "$head" --arg tree "$tree" \
    --arg source_sha "$source_sha" --arg report_sha "$report_sha" \
    --arg policy_sha "$policy_sha" --arg auditor_path "$expected_auditor_path" \
    --arg auditor_sha "$expected_auditor_sha" \
    --arg auditor_version "$expected_auditor_version" \
    --arg auditor_mode "$expected_auditor_mode" \
    --arg auditor_receipt "$expected_auditor_receipt" \
    --arg auditor_receipt_sha "$expected_auditor_receipt_sha" --argjson floor "$floor" '
      keys == ["auditor", "head", "policy", "repo", "report", "schema_version", "source", "status", "tree"] and
      .schema_version == $schema and
      .repo == "jeryu-tool-finder" and .status == "pass" and
      .head == $head and .tree == $tree and
      .source == $source and
      .auditor == {
        path: $auditor_path,
        sha256: $auditor_sha,
        version_output: $auditor_version,
        authority_mode: $auditor_mode,
        installation_receipt: {path: $auditor_receipt, sha256: $auditor_receipt_sha}
      } and
      .report.path == "target/jankurai/repo-score.json" and
      .report.sha256 == $report_sha and
      .policy.path == "agent/audit-policy.toml" and .policy.sha256 == $policy_sha and
      (.report.score | type == "number") and .report.score >= $floor and
      .report.score == $raw[0].score and .report.raw_score == $raw[0].raw_score and
      .report.minimum_score == $floor and .report.caps_applied == [] and
      .report.hard_findings == 0 and
      .report.report_fingerprint == $raw[0].report_fingerprint and
      .report.input_fingerprint == $raw[0].input_fingerprint and
      .report.policy_fingerprint == $raw[0].policy_fingerprint and
      (.report.report_fingerprint | test("^sha256:[0-9a-f]{64}$")) and
      (.report.input_fingerprint | test("^sha256:[0-9a-f]{64}$")) and
      (.report.policy_fingerprint | test("^sha256:[0-9a-f]{64}$"))
    ' "$evidence" >/dev/null || die 'score evidence contract is not satisfied'

  jq -es --argjson minimum "$floor" -f "$score_report_predicate" "$report" \
    >/dev/null || die 'raw score report is not release-ready'
  verify_score_evidence
}

verify_score_evidence() {
  [[ "$(sha_file target/jankurai/evidence.json)" == "$score_evidence_sha" &&
     "$(sha_file target/jankurai/repo-score.json)" == "$score_report_sha" &&
     "$(sha_file agent/audit-policy.toml)" == "$score_policy_sha" ]] ||
    die 'score evidence moved during artifact processing'
  if [[ -n "$expected_auditor_receipt" ]]; then
    [[ "$(sha_file "$expected_auditor_receipt")" == "$expected_auditor_receipt_sha" ]] ||
      die 'Jankurai installation receipt moved during artifact processing'
  fi
}

validate_security() {
  local head="$1" tree="$2" source_sha="$3"
  local evidence='target/security/evidence.json'
  local audit='target/security/cargo-audit.json'
  local sbom='target/security/jeryu-tool-finder.spdx.json'

  require_dir target/security
  require_file "$evidence"
  require_file "$audit"
  require_file "$sbom"
  security_evidence_sha="$(sha_file "$evidence")"
  security_audit_sha="$(sha_file "$audit")"
  security_sbom_sha="$(sha_file "$sbom")"

  jq -e --arg schema "$security_schema" --argjson source "$current_source_json" \
    --arg head "$head" --arg tree "$tree" --arg source_sha "$source_sha" \
    --arg audit_sha "$security_audit_sha" --arg sbom_sha "$security_sbom_sha" '
      keys == ["artifacts", "cargo_audit", "checks", "head", "repo", "sbom", "schema_version", "source", "status", "tree"] and
      .schema_version == $schema and
      .repo == "jeryu-tool-finder" and .status == "pass" and
      .head == $head and .tree == $tree and
      .source == $source and
      .checks == [
        "gitleaks-detect", "actionlint", "env-file", "cargo-metadata",
        "cargo-deny-locked-policy", "cargo-audit-no-fetch", "syft-sbom"
      ] and
      .cargo_audit == "clean" and .sbom == "generated" and
      .artifacts.cargo_audit == {
        path: "target/security/cargo-audit.json", sha256: $audit_sha
      } and
      .artifacts.sbom == {
        path: "target/security/jeryu-tool-finder.spdx.json", sha256: $sbom_sha
      }
    ' "$evidence" >/dev/null || die 'security evidence contract is not release-ready'
  jq -e 'type == "object"' "$audit" >/dev/null || die 'cargo-audit evidence is not JSON'
  jq -e '(.spdxVersion | startswith("SPDX-")) and
         (.SPDXID | type == "string") and (.packages | type == "array")' \
    "$sbom" >/dev/null || die 'SBOM is not a valid SPDX document'
  verify_security_evidence
}

verify_security_evidence() {
  [[ "$(sha_file target/security/evidence.json)" == "$security_evidence_sha" &&
     "$(sha_file target/security/cargo-audit.json)" == "$security_audit_sha" &&
     "$(sha_file target/security/jeryu-tool-finder.spdx.json)" == "$security_sbom_sha" ]] ||
    die 'security evidence moved during artifact processing'
}

validate_release_identity() {
  local version_last_byte metadata
  require_file Cargo.toml
  cargo_lock_path="$repo_root/Cargo.lock"
  rust_toolchain_path="$repo_root/rust-toolchain.toml"
  workspace_manifest_sha=''
  input_prefix=''
  if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
    candidate_root="$(jeryu_candidate_source_root)"
    cargo_lock_path="$candidate_root/Cargo.lock"
    rust_toolchain_path="$candidate_root/rust-toolchain.toml"
    require_physical_file "$candidate_root/Cargo.toml" 'candidate workspace manifest'
    workspace_manifest_sha="$(sha_file "$candidate_root/Cargo.toml")"
    input_prefix='components/jeryu-tool-finder/'
  fi
  require_physical_file "$cargo_lock_path" 'Cargo lockfile'
  require_physical_file "$rust_toolchain_path" 'Rust toolchain'
  require_file VERSION
  require_file contracts/cli-help.txt

  version_last_byte="$(tail -c 1 VERSION | od -An -tu1 | tr -d '[:space:]')"
  [[ "$version_last_byte" == 10 && "$(wc -l < VERSION)" -eq 1 ]] ||
    die 'VERSION must contain exactly one LF-terminated release tag'
  release_tag="$(sed -n '1p' VERSION)"
  [[ "$release_tag" =~ ^jeryu-tool-finder-v([0-9]+\.[0-9]+\.[0-9]+)-split\.([0-9]+)$ ]] ||
    die 'VERSION does not match the Tool Finder split-tag contract'
  version_semver="${BASH_REMATCH[1]}"
  split_revision="${BASH_REMATCH[2]}"

  if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
    metadata="$(run_cargo metadata --locked --offline --format-version 1)"
    jeryu_candidate_require_intelligence_graph "$candidate_root" <<<"$metadata" ||
      die 'candidate artifact dependency graph differs from the verified workspace'
    package_version="$(jq -er --arg root "$repo_root/Cargo.toml" '
      [.packages[] | select(.name == "jeryu-tool-finder" and .manifest_path == $root)]
      | select(length == 1) | .[0].version
    ' <<<"$metadata")"
  else
    metadata="$(run_cargo metadata --locked --offline --format-version 1 --no-deps)" ||
      die 'locked Cargo metadata failed'
    package_version="$(jq -er --arg root "$repo_root/Cargo.toml" '
      select((.packages | length) == 1) |
      .packages[0] |
      select(.name == "jeryu-tool-finder" and .manifest_path == $root) |
      .version
    ' <<<"$metadata")" || die 'Cargo metadata does not describe exactly Tool Finder'
  fi
  [[ "$package_version" == "$version_semver" ]] ||
    die 'Cargo package version differs from VERSION'
  binary_version_output="$binary_name $package_version"

  cargo_toml_sha="$(sha_file Cargo.toml)"
  cargo_lock_sha="$(sha_file "$cargo_lock_path")"
  rust_toolchain_sha="$(sha_file "$rust_toolchain_path")"
  version_file_sha="$(sha_file VERSION)"
  cli_help_sha="$(sha_file contracts/cli-help.txt)"
  receipt_inputs_json="$(jq -cnS --arg prefix "$input_prefix" \
    --arg manifest "$cargo_toml_sha" --arg workspace "$workspace_manifest_sha" \
    --arg lock "$cargo_lock_sha" --arg toolchain "$rust_toolchain_sha" \
    --arg version "$version_file_sha" --arg help "$cli_help_sha" '
    {cargo_manifest:{path:($prefix+"Cargo.toml"),sha256:$manifest},
     cargo_lock:{path:"Cargo.lock",sha256:$lock},
     rust_toolchain:{path:"rust-toolchain.toml",sha256:$toolchain},
     version:{path:($prefix+"VERSION"),sha256:$version},
     cli_help_contract:{path:($prefix+"contracts/cli-help.txt"),sha256:$help}} +
    (if $prefix == "" then {} else
      {workspace_manifest:{path:"Cargo.toml",sha256:$workspace}} end)
  ')"
}

verify_release_inputs() {
  [[ "$(sha_file Cargo.toml)" == "$cargo_toml_sha" &&
     "$(sha_file "$cargo_lock_path")" == "$cargo_lock_sha" &&
     "$(sha_file "$rust_toolchain_path")" == "$rust_toolchain_sha" &&
     "$(sha_file VERSION)" == "$version_file_sha" &&
     "$(sha_file contracts/cli-help.txt)" == "$cli_help_sha" ]] ||
    die 'release identity input moved during artifact processing'
  if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
    [[ "$(sha_file "$candidate_root/Cargo.toml")" == "$workspace_manifest_sha" ]] ||
      die 'candidate workspace manifest moved during artifact processing'
  fi
}

validate_binary_contract() {
  local candidate="$1"
  require_physical_file "$candidate" 'candidate Tool Finder binary'
  [[ -x "$candidate" ]] || die 'candidate Tool Finder binary is not executable'
  help_stdout="$(mktemp "$private_target/.help.stdout.XXXXXX")"
  help_stderr="$(mktemp "$private_target/.help.stderr.XXXXXX")"
  version_stdout="$(mktemp "$private_target/.version.stdout.XXXXXX")"
  version_stderr="$(mktemp "$private_target/.version.stderr.XXXXXX")"
  if ! run_clean_tool "$candidate" --help >"$help_stdout" 2>"$help_stderr"; then
    die 'release CLI --help failed'
  fi
  [[ ! -s "$help_stderr" ]] || die 'release CLI --help wrote stderr'
  if ! cmp -s -- contracts/cli-help.txt "$help_stdout"; then
    diff -u -- contracts/cli-help.txt "$help_stdout" >&2 || true
    die 'release CLI help does not match contracts/cli-help.txt'
  fi
  if ! run_clean_tool "$candidate" --version >"$version_stdout" 2>"$version_stderr"; then
    die 'release CLI --version failed'
  fi
  [[ ! -s "$version_stderr" && "$(<"$version_stdout")" == "$binary_version_output" &&
     "$(wc -l < "$version_stdout")" -eq 1 ]] ||
    die 'release CLI version does not match Cargo.toml and VERSION'
  rm -- "$help_stdout" "$help_stderr" "$version_stdout" "$version_stderr"
  help_stdout=''
  help_stderr=''
  version_stdout=''
  version_stderr=''
}

require_private_build_output() {
  local path="$1" link
  local -a inode_links=()
  [[ "$path" == "$private_target"/* && -f "$path" && ! -L "$path" && -x "$path" ]] ||
    die 'private Cargo output is not an executable regular file in this invocation target'
  [[ "$(realpath -e -- "$path")" == "$path" ]] ||
    die 'private Cargo output contains a path alias'
  mapfile -d '' -t inode_links < <(
    find "$private_target" -xdev -type f -samefile "$path" -print0
  )
  [[ "${#inode_links[@]}" -ge 1 &&
     "${#inode_links[@]}" -eq "$(stat -c '%h' -- "$path")" ]] ||
    die 'private Cargo output has a hard link outside this fresh invocation target'
  for link in "${inode_links[@]}"; do
    [[ "$link" == "$private_target"/* && ! -L "$link" &&
       "$(realpath -e -- "$link")" == "$link" ]] ||
      die 'private Cargo output link custody is invalid'
  done
}

receipt_matches() {
  local receipt_path="$1" binary_path="$2"
  local expected_jobs='null'
  local binary_sha binary_size command_prefix
  if [[ $# -ge 3 ]]; then
    expected_jobs="$3"
  fi
  binary_sha="$(sha_file "$binary_path")"
  binary_size="$(stat -c '%s' -- "$binary_path")"
  command_prefix="cargo build --locked --offline --release --bin $binary_name --jobs "
  jq -e --arg schema "$artifact_schema" --arg status "$artifact_status" \
    --argjson source "$current_source_json" --argjson inputs "$receipt_inputs_json" \
    --argjson configuration "$candidate_cargo_configuration" \
    --arg head "$current_head" --arg tree "$current_tree" \
    --arg source_sha "$current_source_sha" --arg binary_sha "$binary_sha" \
    --argjson binary_size "$binary_size" --arg release_tag "$release_tag" \
    --arg package_version "$package_version" \
    --arg binary_version "$binary_version_output" \
    --argjson split_revision "$split_revision" \
    --arg cargo_toml_sha "$cargo_toml_sha" --arg cargo_lock_sha "$cargo_lock_sha" \
    --arg rust_toolchain_sha "$rust_toolchain_sha" --arg version_sha "$version_file_sha" \
    --arg cli_help_sha "$cli_help_sha" --arg score_sha "$score_evidence_sha" \
    --arg security_sha "$security_evidence_sha" --arg authority_mode "$build_authority_mode" \
    --arg command_prefix "$command_prefix" --arg cargo_path "$cargo_bin" --arg cargo_sha "$cargo_sha" \
    --arg cargo_version "$cargo_version" --arg rustc_path "$rustc_bin" \
    --arg rustc_sha "$rustc_sha" --arg rustc_version "$rustc_version" \
    --arg cargo_config_sha "$cargo_home_config_sha" \
    --arg git_config_sha "$git_global_config_sha" --argjson expected_jobs "$expected_jobs" '
      keys == ["artifact", "build", "evidence", "head", "inputs", "release", "repo", "schema_version", "source", "status", "tree"] and
      .schema_version == $schema and
      .repo == "jeryu-tool-finder" and .status == $status and
      .head == $head and .tree == $tree and
      .source == $source and
      .release == {
        tag: $release_tag,
        package_version: $package_version,
        binary_version_output: $binary_version,
        split_revision: $split_revision
      } and
      .artifact == {
        kind: "rust-cli",
        path: "target/artifact-support/jeryu-tool-finder",
        sha256: $binary_sha,
        size: $binary_size,
        mode: "0555"
      } and
      .inputs == $inputs and
      .evidence == {
        score: {path: "target/jankurai/evidence.json", sha256: $score_sha},
        security: {path: "target/security/evidence.json", sha256: $security_sha}
      } and
      (.build.jobs as $receipt_jobs |
        ($receipt_jobs |
          if type == "number" then . >= 1 and . <= (if $configuration == null then 256 else 2 end) and . == floor else false end) and
        ($expected_jobs == null or $receipt_jobs == $expected_jobs) and
        .build == ({
          authority_mode: $authority_mode,
          command: ($command_prefix + ($receipt_jobs | tostring)),
          profile: "release",
          jobs: $receipt_jobs,
          target_policy: "fresh-private-target-no-preseed-v1",
          cargo: {path: $cargo_path, sha256: $cargo_sha, version_output: $cargo_version},
          rustc: {path: $rustc_path, sha256: $rustc_sha, version_output: $rustc_version},
          cargo_home_config_sha256: $cargo_config_sha,
          git_global_config_sha256: $git_config_sha
        } | if $configuration == null then . else
          del(.cargo_home_config_sha256) + {cargo_configuration:$configuration} end))
    ' "$receipt_path" >/dev/null
}

set_current_source() {
  jeryu_source_snapshot
  current_head="$JERYU_SOURCE_HEAD"
  current_tree="$JERYU_SOURCE_TREE"
  current_source_sha="$JERYU_SOURCE_INPUTS_SHA256"
  current_source_json="$(jq -cnS --arg sha "$current_source_sha" \
    --argjson scope "$JERYU_SOURCE_SCOPE_JSON" '
    {tracked_inputs_sha256:$sha} +
      (if $scope == null then {} else {scope:$scope} end)
  ')"
}

validate_prerequisites() {
  resolve_auditor_authority
  validate_release_identity
  validate_score "$current_head" "$current_tree" "$current_source_sha"
  validate_security "$current_head" "$current_tree" "$current_source_sha"
  verify_release_inputs
  verify_build_authority
  jeryu_source_verify "$current_head" "$current_tree" "$current_source_sha"
}

validate_receipt() {
  local receipt_sha artifact_sha
  set_current_source
  validate_prerequisites
  require_file "$receipt_rel"
  require_file "$artifact_rel"
  [[ "$(stat -c '%a' -- "$artifact")" == 555 ]] ||
    die 'support artifact mode is not 0555'
  receipt_sha="$(sha_file "$receipt")"
  artifact_sha="$(sha_file "$artifact")"
  validate_binary_contract "$artifact"
  receipt_matches "$receipt" "$artifact" ||
    die 'artifact-support receipt does not bind current release identity and evidence'
  verify_score_evidence
  verify_security_evidence
  verify_release_inputs
  verify_build_authority
  jeryu_source_verify "$current_head" "$current_tree" "$current_source_sha"
  [[ "$(sha_file "$receipt")" == "$receipt_sha" &&
     "$(sha_file "$artifact")" == "$artifact_sha" ]] ||
    die 'artifact-support output moved during validation'
}

produce_receipt() {
  local raw_build build build_sha binary_sha binary_size command
  prepare_output_dirs
  set_current_source
  validate_prerequisites

  raw_build="$private_target/release/$binary_name"
  [[ ! -e "$raw_build" && ! -L "$raw_build" ]] ||
    die 'fresh private target was preseeded before the build'
  run_cargo build --locked --offline --release --bin "$binary_name" --jobs "$build_jobs"
  verify_build_authority
  verify_score_evidence
  verify_security_evidence
  verify_release_inputs
  jeryu_source_verify "$current_head" "$current_tree" "$current_source_sha"
  require_private_build_output "$raw_build"
  build_sha="$(sha_file "$raw_build")"
  mkdir -m 0700 -- "$private_target/materialized"
  build="$private_target/materialized/$binary_name"
  cp --reflink=never -- "$raw_build" "$build"
  chmod 0555 "$build"
  require_physical_file "$build" 'single-link private build materialization'
  [[ "$(sha_file "$build")" == "$build_sha" &&
     "$(sha_file "$raw_build")" == "$build_sha" ]] ||
    die 'private Cargo output moved during single-link materialization'
  validate_binary_contract "$build"

  artifact_tmp_dir="$(mktemp -d "$repo_root/target/artifact-support/.candidate.XXXXXX")"
  chmod 0700 "$artifact_tmp_dir"
  artifact_tmp_identity=$(
    jeryu_record_test_scratch "$artifact_tmp_dir" || exit 1
    printf '%s\n' "${jeryu_test_scratch_identity:?}"
  )
  artifact_tmp="$artifact_tmp_dir/$binary_name"
  cp --reflink=never -- "$build" "$artifact_tmp"
  chmod 0555 "$artifact_tmp"
  require_physical_file "$artifact_tmp" 'candidate support artifact'
  [[ "$(stat -c '%i' -- "$artifact_tmp")" != "$(stat -c '%i' -- "$build")" &&
     "$(sha_file "$artifact_tmp")" == "$build_sha" &&
     "$(sha_file "$build")" == "$build_sha" ]] ||
    die 'support artifact was not copied from this private build invocation'
  validate_binary_contract "$artifact_tmp"

  binary_sha="$(sha_file "$artifact_tmp")"
  binary_size="$(stat -c '%s' -- "$artifact_tmp")"
  command="cargo build --locked --offline --release --bin $binary_name --jobs $build_jobs"
  receipt_tmp="$(mktemp "$artifact_tmp_dir/receipt.XXXXXX")"
  jq -nS \
    --arg schema_version "$artifact_schema" \
    --arg repo 'jeryu-tool-finder' --arg status "$artifact_status" \
    --argjson source "$current_source_json" --argjson inputs "$receipt_inputs_json" \
    --argjson configuration "$candidate_cargo_configuration" \
    --arg head "$current_head" --arg tree "$current_tree" \
    --arg source_sha "$current_source_sha" --arg binary_sha "$binary_sha" \
    --argjson binary_size "$binary_size" --arg release_tag "$release_tag" \
    --arg package_version "$package_version" --arg binary_version "$binary_version_output" \
    --argjson split_revision "$split_revision" --arg cargo_toml_sha "$cargo_toml_sha" \
    --arg cargo_lock_sha "$cargo_lock_sha" --arg rust_toolchain_sha "$rust_toolchain_sha" \
    --arg version_sha "$version_file_sha" --arg cli_help_sha "$cli_help_sha" \
    --arg score_sha "$score_evidence_sha" --arg security_sha "$security_evidence_sha" \
    --arg authority_mode "$build_authority_mode" --arg command "$command" \
    --arg cargo_path "$cargo_bin" --arg cargo_sha "$cargo_sha" \
    --arg cargo_version "$cargo_version" --arg rustc_path "$rustc_bin" \
    --arg rustc_sha "$rustc_sha" --arg rustc_version "$rustc_version" \
    --arg cargo_config_sha "$cargo_home_config_sha" \
    --arg git_config_sha "$git_global_config_sha" --argjson jobs "$build_jobs" '
    {
      schema_version: $schema_version,
      repo: $repo,
      status: $status,
      head: $head,
      tree: $tree,
      source: $source,
      release: {
        tag: $release_tag,
        package_version: $package_version,
        binary_version_output: $binary_version,
        split_revision: $split_revision
      },
      artifact: {
        kind: "rust-cli",
        path: "target/artifact-support/jeryu-tool-finder",
        sha256: $binary_sha,
        size: $binary_size,
        mode: "0555"
      },
      inputs: $inputs,
      evidence: {
        score: {path: "target/jankurai/evidence.json", sha256: $score_sha},
        security: {path: "target/security/evidence.json", sha256: $security_sha}
      },
      build: ({
        authority_mode: $authority_mode,
        command: $command,
        profile: "release",
        jobs: $jobs,
        target_policy: "fresh-private-target-no-preseed-v1",
        cargo: {path: $cargo_path, sha256: $cargo_sha, version_output: $cargo_version},
        rustc: {path: $rustc_path, sha256: $rustc_sha, version_output: $rustc_version},
        cargo_home_config_sha256: $cargo_config_sha,
        git_global_config_sha256: $git_config_sha
      } | if $configuration == null then . else
        del(.cargo_home_config_sha256) + {cargo_configuration:$configuration} end)
    }
  ' >"$receipt_tmp"
  chmod 0600 "$receipt_tmp"

  receipt_matches "$receipt_tmp" "$artifact_tmp" "$build_jobs" ||
    die 'candidate artifact receipt is internally inconsistent'
  verify_score_evidence
  verify_security_evidence
  verify_release_inputs
  verify_build_authority
  jeryu_source_verify "$current_head" "$current_tree" "$current_source_sha"
  [[ "$(sha_file "$raw_build")" == "$build_sha" &&
     "$(sha_file "$build")" == "$build_sha" &&
     "$(sha_file "$artifact_tmp")" == "$binary_sha" ]] ||
    die 'candidate artifact moved before publication'

  # Both prior outputs remain untouched until every prerequisite and candidate
  # check above has passed. These final same-directory renames publish only the
  # new complete pair; no early failure can destroy a prior valid receipt.
  mv -- "$artifact_tmp" "$artifact"
  artifact_tmp=''
  mv -- "$receipt_tmp" "$receipt"
  receipt_tmp=''
  jeryu_test_scratch="$artifact_tmp_dir" \
    jeryu_test_scratch_identity="$artifact_tmp_identity" jeryu_remove_test_scratch ||
    die 'published artifact scratch directory changed custody'
  artifact_tmp_dir=''
  validate_receipt
  cleanup 0 || die 'artifact build scratch cleanup failed'
  printf 'artifact-support %s: %s\n' "$artifact_status" "$receipt_rel"
}

prepare_output_dirs
validate_build_environment

case "$#:${1-}" in
  0:)
    produce_receipt
    ;;
  1:--validate-receipt)
    validate_receipt
    cleanup 0 || die 'artifact validation scratch cleanup failed'
    printf 'artifact-support receipt valid: %s\n' "$receipt_rel"
    ;;
  *)
    printf 'usage: artifact-support.sh [--validate-receipt]\n' >&2
    exit 2
    ;;
esac
