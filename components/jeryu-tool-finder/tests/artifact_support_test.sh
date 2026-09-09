#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
validator="$repo_root/ops/ci/artifact-support.sh"
receipt="$repo_root/target/artifact-support/jeryu-tool-finder.json"
artifact="$repo_root/target/artifact-support/jeryu-tool-finder"
score="$repo_root/target/jankurai/evidence.json"
security="$repo_root/target/security/evidence.json"
test_dir="$repo_root/target/artifact-support"
stderr_log="$test_dir/.hostile.stderr"
# shellcheck source=/dev/null
source "$repo_root/tests/scratch.sh"
hostile_root="$(mktemp -d "$test_dir/.hostiles.XXXXXX")"
jeryu_record_test_scratch "$hostile_root"
external_target=''
external_target_identity=''
fake_bin="$hostile_root/fake-bin"
fake_marker="$hostile_root/fake-tool-executed"
stale_marker="$hostile_root/stale-repo-target-executed"
stale_build="$repo_root/target/release/jeryu-tool-finder"
receipt_saved=''
artifact_saved=''
security_saved=''
score_saved=''
stale_saved=''
stale_owned=false
receipt_jobs=''
alternate_jobs=''

cleanup() {
  local status=$?
  rm -f -- "$test_dir/.receipt-hardlink" \
    "$test_dir/.artifact-hardlink" \
    "$repo_root/target/jankurai/.evidence-hardlink" \
    "$repo_root/target/security/.evidence-hardlink" "$stderr_log" || exit 1
  if [[ -n "$score_saved" && -f "$score_saved" ]]; then
    rm -f -- "$score" || exit 1
    mv -- "$score_saved" "$score" || exit 1
  fi
  if [[ -L "$receipt" ]]; then
    rm -- "$receipt" || exit 1
  fi
  if [[ -n "$receipt_saved" && -f "$receipt_saved" ]]; then
    rm -f -- "$receipt" || exit 1
    mv -- "$receipt_saved" "$receipt" || exit 1
  fi
  if [[ -L "$artifact" ]]; then
    rm -- "$artifact" || exit 1
  fi
  if [[ -n "$artifact_saved" && -f "$artifact_saved" ]]; then
    rm -f -- "$artifact" || exit 1
    mv -- "$artifact_saved" "$artifact" || exit 1
  fi
  if [[ -n "$security_saved" && -f "$security_saved" ]]; then
    rm -f -- "$security" || exit 1
    mv -- "$security_saved" "$security" || exit 1
  fi
  if [[ -n "$stale_saved" && -f "$stale_saved" ]]; then
    rm -f -- "$stale_build" || exit 1
    mv -- "$stale_saved" "$stale_build" || exit 1
  elif [[ "$stale_owned" == true && -f "$stale_build" ]]; then
    rm -- "$stale_build" || exit 1
  fi
  if [[ -n "$external_target" ]]; then
    jeryu_test_scratch="$external_target" \
      jeryu_test_scratch_identity="$external_target_identity" \
      jeryu_remove_test_scratch || status=1
  fi
  jeryu_remove_test_scratch || status=1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
external_target="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-tool-finder-external-target.XXXXXX")"
external_target_identity="$(
  jeryu_record_test_scratch "$external_target" || exit 1
  printf '%s\n' "${jeryu_test_scratch_identity:?}"
)"

fail() {
  printf 'artifact-support hostile test failed: %s\n' "$1" >&2
  sed -n '1,12p' "$stderr_log" >&2
  exit 1
}

expect_invalid() {
  if bash "$validator" --validate-receipt > /dev/null 2> "$stderr_log"; then
    fail "$1"
  fi
}

pair_identity() {
  require_pair_path "$artifact"
  require_pair_path "$receipt"
  printf '%s %s\n' \
    "$(sha256sum -- "$artifact" | awk '{print $1}'):$(stat -c '%d:%i:%a:%h:%s:%Y' -- "$artifact")" \
    "$(sha256sum -- "$receipt" | awk '{print $1}'):$(stat -c '%d:%i:%a:%h:%s:%Y' -- "$receipt")"
}

file_identity() {
  local path="$1"
  printf '%s:%s\n' "$(sha256sum -- "$path" | awk '{print $1}')" \
    "$(stat -c '%d:%i:%a:%h:%s:%Y' -- "$path")"
}

require_pair_path() {
  local path="$1"
  [[ -f "$path" && ! -L "$path" ]] || fail "published pair member is unsafe: $path"
}

expect_generation_rejected() {
  local label="$1" before after
  shift
  before="$(pair_identity)"
  if env "$@" bash "$validator" > /dev/null 2> "$stderr_log"; then
    fail "$label passed artifact generation"
  fi
  after="$(pair_identity)"
  [[ "$after" == "$before" ]] || fail "$label changed the prior artifact/receipt pair"
  [[ ! -e "$fake_marker" ]] || fail "$label executed a fake build tool"
  [[ -z "$(find "$repo_root/target/artifact-support-build" -mindepth 1 -maxdepth 1 \
    -name '.jeryu-tool-finder.*' -print -quit 2>/dev/null)" ]] ||
    fail "$label leaked a private target"
}

tamper_receipt() {
  local label="$1" filter="$2"
  receipt_saved="$test_dir/.receipt-saved"
  mv -- "$receipt" "$receipt_saved"
  jq "$filter" "$receipt_saved" > "$receipt"
  chmod 0600 "$receipt"
  expect_invalid "$label"
  rm -- "$receipt"
  mv -- "$receipt_saved" "$receipt"
  receipt_saved=''
  bash "$validator" --validate-receipt >/dev/null
}

tamper_evidence() {
  local label="$1" evidence="$2" filter="$3" receipt_field="$4"
  local evidence_sha
  receipt_saved="$test_dir/.receipt-saved"
  if [[ "$evidence" == "$score" ]]; then
    score_saved="$repo_root/target/jankurai/.evidence-saved"
    mv -- "$score" "$score_saved"
    jq "$filter" "$score_saved" > "$score"
  else
    security_saved="$repo_root/target/security/.evidence-saved"
    mv -- "$security" "$security_saved"
    jq "$filter" "$security_saved" > "$security"
  fi
  chmod 0600 "$evidence"
  evidence_sha="$(sha256sum -- "$evidence" | awk '{print $1}')"
  mv -- "$receipt" "$receipt_saved"
  jq --arg digest "$evidence_sha" "$receipt_field.sha256 = \$digest" \
    "$receipt_saved" > "$receipt"
  chmod 0600 "$receipt"
  expect_invalid "$label"
  rm -- "$receipt" "$evidence"
  mv -- "$receipt_saved" "$receipt"
  receipt_saved=''
  if [[ -n "$score_saved" ]]; then
    mv -- "$score_saved" "$score"
    score_saved=''
  else
    mv -- "$security_saved" "$security"
    security_saved=''
  fi
  bash "$validator" --validate-receipt >/dev/null
}

bash "$validator" --validate-receipt >/dev/null
receipt_jobs="$(jq -er '.build.jobs' "$receipt")"
if [[ "$receipt_jobs" == 1 ]]; then
  alternate_jobs=2
else
  alternate_jobs=1
fi
JERYU_CI_JOBS="$alternate_jobs" CARGO_BUILD_JOBS="$alternate_jobs" \
  bash "$validator" --validate-receipt >/dev/null

ln -- "$artifact" "$test_dir/.artifact-hardlink"
expect_invalid 'multiply linked support artifact passed validation'
expect_generation_rejected 'multiply linked prior artifact'
rm -- "$test_dir/.artifact-hardlink"
bash "$validator" --validate-receipt >/dev/null

artifact_saved="$test_dir/.artifact-saved"
mv -- "$artifact" "$artifact_saved"
ln -s -- "$(basename "$artifact_saved")" "$artifact"
expect_invalid 'symlinked support artifact passed validation'
rm -- "$artifact"
mv -- "$artifact_saved" "$artifact"
artifact_saved=''
bash "$validator" --validate-receipt >/dev/null

ln -- "$receipt" "$test_dir/.receipt-hardlink"
expect_invalid 'multiply linked receipt passed validation'
expect_generation_rejected 'multiply linked prior receipt'
rm -- "$test_dir/.receipt-hardlink"
bash "$validator" --validate-receipt >/dev/null

receipt_saved="$test_dir/.receipt-saved"
mv -- "$receipt" "$receipt_saved"
ln -s -- "$(basename "$receipt_saved")" "$receipt"
expect_invalid 'symlinked receipt passed validation'
rm -- "$receipt"
mv -- "$receipt_saved" "$receipt"
receipt_saved=''
bash "$validator" --validate-receipt >/dev/null

tamper_receipt 'non-ready receipt passed validation' '.status = "bootstrap"'
tamper_receipt 'release tag mismatch passed validation' '.release.tag = "jeryu-tool-finder-v9.9.9-split.9"'
tamper_receipt 'Cargo package version mismatch passed validation' '.release.package_version = "9.9.9"'
tamper_receipt 'binary version mismatch passed validation' '.release.binary_version_output = "jeryu-tool-finder 9.9.9"'
tamper_receipt 'Cargo.toml digest mismatch passed validation' '.inputs.cargo_manifest.sha256 = ("0" * 64)'
tamper_receipt 'rust-toolchain digest mismatch passed validation' '.inputs.rust_toolchain.sha256 = ("0" * 64)'
tamper_receipt 'VERSION digest mismatch passed validation' '.inputs.version.sha256 = ("0" * 64)'
tamper_receipt 'source digest mismatch passed validation' '.source.tracked_inputs_sha256 = ("0" * 64)'
tamper_receipt 'Cargo tool digest mismatch passed validation' '.build.cargo.sha256 = ("0" * 64)'
tamper_receipt 'private-target policy mismatch passed validation' '.build.target_policy = "repo-target"'
tamper_receipt 'out-of-range recorded build jobs passed validation' \
  '.build.jobs = 0 | .build.command = "cargo build --locked --offline --release --bin jeryu-tool-finder --jobs 0"'
tamper_receipt 'build command and recorded jobs mismatch passed validation' \
  '.build.command += " "'

ln -- "$score" "$repo_root/target/jankurai/.evidence-hardlink"
expect_invalid 'multiply linked score evidence passed validation'
score_before="$(file_identity "$score")"
if bash "$repo_root/ops/ci/score.sh" > /dev/null 2> "$stderr_log"; then
  fail 'score producer accepted a multiply linked prior receipt'
fi
[[ "$(file_identity "$score")" == "$score_before" ]] ||
  fail 'failed score generation changed its prior receipt'
rm -- "$repo_root/target/jankurai/.evidence-hardlink"
bash "$validator" --validate-receipt >/dev/null

ln -- "$security" "$repo_root/target/security/.evidence-hardlink"
security_before="$(file_identity "$security")"
if bash "$repo_root/ops/ci/security.sh" > /dev/null 2> "$stderr_log"; then
  fail 'security producer accepted a multiply linked prior receipt'
fi
[[ "$(file_identity "$security")" == "$security_before" ]] ||
  fail 'failed security generation changed its prior receipt'
rm -- "$repo_root/target/security/.evidence-hardlink"
bash "$validator" --validate-receipt >/dev/null

security_saved="$repo_root/target/security/.evidence-saved"
mv -- "$security" "$security_saved"
expect_invalid 'missing security evidence passed validation'
mv -- "$security_saved" "$security"
security_saved=''
bash "$validator" --validate-receipt >/dev/null

tamper_evidence 'score source substitution passed semantic validation' "$score" \
  '.source.tracked_inputs_sha256 = ("0" * 64)' '.evidence.score'
tamper_evidence 'score auditor substitution passed semantic validation' "$score" \
  '.auditor.path = "/bin/false"' '.evidence.score'
tamper_evidence 'security source substitution passed semantic validation' "$security" \
  '.source.tracked_inputs_sha256 = ("0" * 64)' '.evidence.security'

mkdir -p -- "$external_target/release" "$fake_bin"
{
  printf '%s\n' '#!/usr/bin/env bash'
  printf 'printf fake > %q\n' "$fake_marker"
  printf '%s\n' 'exit 0'
} > "$external_target/release/jeryu-tool-finder"
chmod 0755 "$external_target/release/jeryu-tool-finder"
for tool in cargo rustc fake-wrapper fake-linker; do
  {
    printf '%s\n' '#!/usr/bin/env bash'
    printf 'printf fake > %q\n' "$fake_marker"
    printf '%s\n' 'exit 0'
  } > "$fake_bin/$tool"
  chmod 0755 "$fake_bin/$tool"
done

expect_generation_rejected 'external preseeded CARGO_TARGET_DIR' \
  CARGO_TARGET_DIR="$external_target"
expect_generation_rejected 'fake PATH cargo and rustc' PATH="$fake_bin:$PATH"
for hostile in \
  "CARGO=$fake_bin/cargo" \
  "RUSTC=$fake_bin/rustc" \
  "RUSTC_WRAPPER=$fake_bin/fake-wrapper" \
  "RUSTC_WORKSPACE_WRAPPER=$fake_bin/fake-wrapper" \
  'RUSTFLAGS=-C opt-level=0' \
  'CARGO_PROFILE_RELEASE_LTO=false' \
  'CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu' \
  "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=$fake_bin/fake-linker"; do
  expect_generation_rejected "hostile build override ${hostile%%=*}" "$hostile"
done

# A rejected prerequisite with no prior pair must not synthesize readiness.
artifact_saved="$test_dir/.artifact-saved"
receipt_saved="$test_dir/.receipt-saved"
mv -- "$artifact" "$artifact_saved"
mv -- "$receipt" "$receipt_saved"
if CARGO_TARGET_DIR="$external_target" bash "$validator" > /dev/null 2> "$stderr_log"; then
  fail 'external target passed with no prior pair'
fi
[[ ! -e "$artifact" && ! -e "$receipt" ]] ||
  fail 'failed prerequisite synthesized a ready artifact or receipt'
mv -- "$artifact_saved" "$artifact"
mv -- "$receipt_saved" "$receipt"
artifact_saved=''
receipt_saved=''

# A stale ignored repo-local release output is neither executed nor consumed;
# the producer must materialize a new binary from its private invocation target.
mkdir -p -- "$(dirname "$stale_build")"
if [[ -e "$stale_build" || -L "$stale_build" ]]; then
  [[ -f "$stale_build" && ! -L "$stale_build" ]] || fail 'existing repo-local release output is unsafe'
  stale_saved="$test_dir/.stale-release-saved"
  mv -- "$stale_build" "$stale_saved"
fi
{
  printf '%s\n' '#!/usr/bin/env bash'
  printf 'printf stale > %q\n' "$stale_marker"
  printf '%s\n' 'exit 0'
} > "$stale_build"
chmod 0755 "$stale_build"
stale_owned=true
bash "$validator" >/dev/null
[[ ! -e "$stale_marker" ]] || fail 'repo-local stale release binary was executed'
[[ "$("$artifact" --version)" == 'jeryu-tool-finder 5.1.0' ]] ||
  fail 'private build did not publish the truthful Tool Finder version'
rm -- "$stale_build"
stale_owned=false
if [[ -n "$stale_saved" ]]; then
  mv -- "$stale_saved" "$stale_build"
  stale_saved=''
fi
bash "$validator" --validate-receipt >/dev/null

printf 'artifact-support hostile tests ok\n'
