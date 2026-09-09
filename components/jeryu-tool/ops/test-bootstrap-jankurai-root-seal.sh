#!/usr/bin/env bash
# Hostile tests for the one-attempt, receipt-bound root-seal bootstrap.
set -euo pipefail
umask 077

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source_bootstrap="${here}/bootstrap-jankurai-root-seal.sh"
canonical_pin="${here}/../generated/jankurai-pin.env"
canonical_candidate="$(
  sed -n 's/^JANKURAI_BINARY_SHA256="\([0-9a-f]\{64\}\)"$/\1/p' "$canonical_pin"
)"
production_candidate="$(
  sed -n 's/^readonly production_candidate_sha256="\([0-9a-f]\{64\}\)"$/\1/p' \
    "$source_bootstrap"
)"
[[ -n "$canonical_candidate" && "$production_candidate" == "$canonical_candidate" ]] || {
  printf 'test-bootstrap-jankurai-root-seal: production candidate differs from canonical pin\n' >&2
  exit 1
}
tmp="$(mktemp -d /tmp/test-bootstrap-jankurai-root-seal.XXXXXX)"
# shellcheck source=ops/test-scratch.sh
source "${here}/test-scratch.sh"
record_test_scratch "${tmp}"
cleanup() {
  local status=$?
  remove_test_scratch || status=1
  exit "${status}"
}
trap cleanup EXIT

fail() {
  printf 'test-bootstrap-jankurai-root-seal: %s\n' "$*" >&2
  exit 1
}

expect_failure() {
  local description="$1" pattern="$2"
  shift 2
  if "$@" >"$tmp/failure.log" 2>&1; then
    fail "$description unexpectedly succeeded"
  fi
  grep -Fq "$pattern" "$tmp/failure.log" || {
    sed -n '1,120p' "$tmp/failure.log" >&2
    fail "$description did not report the expected failure"
  }
}

repo="$tmp/repo"
remote="$tmp/jeryu-tool.git"
splitops_repo="$tmp/splitops-repo"
splitops_remote="$tmp/jain-split-ops.git"
install="$tmp/install"
state="$tmp/state"
evidence="$tmp/evidence"
mkdir -p "$repo" "$splitops_repo" "$install" "$evidence"
git init -q --bare "$remote"
git -C "$repo" init -q
git -C "$repo" config user.name 'Bootstrap Test'
git -C "$repo" config user.email bootstrap-test@example.invalid
git -C "$repo" checkout -q -b codex/jeryu-tool-jankurai-split3-production-digest-r17-20260802
printf 'fixture manifest\n' >"$repo/tool-manifest.toml"
mkdir -p "$repo/ops" "$repo/generated"
cp "$source_bootstrap" "$repo/ops/bootstrap-jankurai-root-seal.sh"
chmod 0755 "$repo/ops/bootstrap-jankurai-root-seal.sh"
control_ref=refs/heads/codex/jeryu-tool-jankurai-split3-production-digest-r17-20260802

candidate="$evidence/jankurai"
cat >"$candidate" <<'CANDIDATE'
#!/usr/bin/env bash
printf 'jankurai test-candidate\n'
CANDIDATE
chmod 0755 "$candidate"
candidate_sha="$(sha256sum "$candidate" | awk '{print $1}')"

predecessor_source="$tmp/predecessor"
cat >"$predecessor_source" <<'PREDECESSOR'
#!/usr/bin/env bash
printf 'jankurai protected-predecessor\n'
PREDECESSOR
chmod 0555 "$predecessor_source"
predecessor_sha="$(sha256sum "$predecessor_source" | awk '{print $1}')"
cp "$predecessor_source" "$install/jankurai"
chmod 0555 "$install/jankurai"

pin_source="$repo/generated/jankurai-pin.env"
pin_env="$install/jeryu-tool-root-seal-pin.env"
cat >"$pin_env" <<EOF
JANKURAI_REPO="fixture://jankurai"
JANKURAI_VERSION="jankurai test-candidate"
JANKURAI_BINARY_SHA256="$candidate_sha"
JANKURAI_REV="$(printf '1%.0s' {1..40})"
JANKURAI_TAG="v-test"
JANKURAI_SOURCE_TREE="$(printf '2%.0s' {1..40})"
JANKURAI_SOURCE_ARCHIVE_SHA256="$(printf '3%.0s' {1..64})"
JANKURAI_CARGO_LOCK_SHA256="$(printf '4%.0s' {1..64})"
JANKURAI_RUSTC_VERSION="rustc test"
JANKURAI_CARGO_VERSION="cargo test"
JANKURAI_TARGET_TRIPLE="x86_64-unknown-linux-gnu"
JANKURAI_BUILD_MODE="test-closed-build"
JANKURAI_PACKAGE_PATH="crates/jankurai"
JANKURAI_BUILDER_IMAGE="fixture@sha256:$(printf '5%.0s' {1..64})"
JANKURAI_BUILDER_IMAGE_ID="sha256:$(printf '5%.0s' {1..64})"
JANKURAI_LINKER_VERSION="ld test"
JANKURAI_GLIBC_VERSION="glibc test"
JANKURAI_VENDOR_FILES_SHA256="$(printf '6%.0s' {1..64})"
JANKURAI_VENDOR_FILE_COUNT="1"
JANKURAI_CARGO_CONFIG_SHA256="$(printf '7%.0s' {1..64})"
JANKURAI_BUILD_ENVIRONMENT="offline"
JANKURAI_RUSTFLAGS="--test"
JANKURAI_BUILD_COMMAND="cargo test"
JANKURAI_BUILD_CONTEXT_SHA256="$(printf '8%.0s' {1..64})"
EOF
cp "$pin_env" "$pin_source"
chmod 0400 "$pin_env"
# shellcheck disable=SC1090
source "$pin_env"

git -C "$repo" add tool-manifest.toml ops/bootstrap-jankurai-root-seal.sh \
  generated/jankurai-pin.env
git -C "$repo" commit -q -m 'test: bootstrap fixture'
git -C "$repo" remote add origin "$remote"
git -C "$repo" push -q -u origin HEAD
head_sha="$(git -C "$repo" rev-parse HEAD)"
tree_sha="$(git -C "$repo" rev-parse 'HEAD^{tree}')"
manifest_sha="$(sha256sum "$repo/tool-manifest.toml" | awk '{print $1}')"
git -C "$repo" remote set-url origin \
  http://127.0.0.1:8787/git/jeryu/jeryu-tool.git

for kind in publisher sandbox; do
  jq -n --arg schema "test-$kind/v1" --arg digest "$predecessor_sha" \
    '{schema_version:$schema,jankurai_sha256:$digest,
      retained_authority:"protected-main"}' \
    >"$install/host-ci-$kind.config.json"
  chmod 0600 "$install/host-ci-$kind.config.json"
done
publisher="$install/host-ci-publisher.config.json"
sandbox="$install/host-ci-sandbox.config.json"
publisher_original_sha="$(sha256sum "$publisher" | awk '{print $1}')"
sandbox_original_sha="$(sha256sum "$sandbox" | awk '{print $1}')"

git init -q --bare "$splitops_remote"
git -C "$splitops_repo" init -q
git -C "$splitops_repo" config user.name 'SplitOps Bootstrap Test'
git -C "$splitops_repo" config user.email splitops-test@example.invalid
git -C "$splitops_repo" checkout -q -b main
mkdir -p "$splitops_repo/ops/ci"
runner="$splitops_repo/ops/ci/split-host-ci.sh"
cat >"$runner" <<'RUNNER'
#!/usr/bin/env bash
set -euo pipefail
ops_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ops_root/ops/ci/split-host-ci-parent.sh" "$@"
RUNNER
parent="$splitops_repo/ops/ci/split-host-ci-parent.sh"
cat >"$parent" <<'PARENT'
#!/usr/bin/env bash
set -euo pipefail
ops_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
[[ "$("$ops_root/ops/ci/host-ci-integrity.sh" "$ops_root" \
  --ref refs/remotes/origin/main)" == "$MOCK_SPLITOPS_COMMIT" ]]
[[ "$#" == 5 && "$1" == jeryu && "$2" == jeryu-tool \
  && "$3" == "$MOCK_HEAD" && "$4" == "$JERYU_BOOTSTRAP_REPO_ROOT" \
  && "$5" == jeryu-tool/required ]]
[[ "$(sha256sum "$JERYU_BOOTSTRAP_INSTALL_DIR/jankurai" | awk '{print $1}')" \
  == "$MOCK_CANDIDATE_SHA" ]]
[[ "$(jq -er '.jankurai_sha256' \
  "$JERYU_BOOTSTRAP_INSTALL_DIR/host-ci-publisher.config.json")" \
  == "$MOCK_CANDIDATE_SHA" ]]
[[ "$(jq -er '.jankurai_sha256' \
  "$JERYU_BOOTSTRAP_INSTALL_DIR/host-ci-sandbox.config.json")" \
  == "$MOCK_CANDIDATE_SHA" ]]
if [[ -n "${MOCK_READY_FILE:-}" ]]; then
  : >"$MOCK_READY_FILE"
fi
if [[ -n "${MOCK_SLEEP_SECONDS:-}" ]]; then
  sleep "$MOCK_SLEEP_SECONDS"
fi
exit "${MOCK_EXIT_CODE:-0}"
PARENT
integrity="$splitops_repo/ops/ci/host-ci-integrity.sh"
cat >"$integrity" <<'INTEGRITY'
#!/usr/bin/env bash
set -euo pipefail
[[ "$#" == 3 && "$2" == --ref ]]
git -c safe.directory="$1" -c core.fsmonitor=false \
  -c core.hooksPath=/dev/null -C "$1" rev-parse --verify "$3^{commit}"
INTEGRITY
chmod 0755 "$runner" "$parent" "$integrity"
git -C "$splitops_repo" add ops/ci/split-host-ci.sh \
  ops/ci/split-host-ci-parent.sh ops/ci/host-ci-integrity.sh
git -C "$splitops_repo" commit -q -m 'test: protected splitops runner'
splitops_commit="$(git -C "$splitops_repo" rev-parse HEAD)"
splitops_tag=refs/tags/jain-split-ops-v10.0.0-split.15
git -C "$splitops_repo" tag "${splitops_tag#refs/tags/}" "$splitops_commit"
git -C "$splitops_repo" remote add origin "$splitops_remote"
git -C "$splitops_repo" push -q origin main "${splitops_tag#refs/tags/}"

bootstrap="$install/bootstrap-jankurai-root-seal"
authority_config="$install/jeryu-tool-root-seal.config.json"
splitops_config="$install/native-build-tools-installer.config.json"
splitctl="$install/splitctl"
token_file="$install/jeryu-merge-token"
cp "$source_bootstrap" "$bootstrap"
chmod 0500 "$bootstrap"
printf 'test-token\n' >"$token_file"
chmod 0600 "$token_file"
cat >"$splitctl" <<'SPLITCTL'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == jeryu-local ]]
command="$2"
shift 2
value() {
  local flag="$1"
  shift
  while [[ "$#" -gt 1 ]]; do
    if [[ "$1" == "$flag" ]]; then
      printf '%s\n' "$2"
      return 0
    fi
    shift
  done
  return 1
}
case "$command" in
  protection-readback)
    [[ "${MOCK_PROTECTION_MISMATCH:-0}" != 1 ]] || exit 41
    printf '{"status":"pass"}\n'
    ;;
  ref-readback)
    repo="$(value --repo "$@")"
    reference="$(value --ref "$@")"
    expected="$(value --expected-head "$@")"
    if [[ "$repo" == jeryu/jeryu-tool ]]; then
      actual="$(git --git-dir="$MOCK_JERYU_REMOTE_PATH" rev-parse "$reference")"
    else
      actual="$(git --git-dir="$MOCK_SPLITOPS_REMOTE_PATH" rev-parse "$reference")"
    fi
    [[ "$actual" == "$expected" ]]
    printf '{"status":"pass"}\n'
    ;;
  git-materialize)
    repo="$(value --repo "$@")"
    remote="$(value --remote "$@")"
    reference="$(value --ref "$@")"
    destination="$(value --destination "$@")"
    if [[ "$repo" == jeryu/jeryu-tool ]]; then
      source_remote="$MOCK_JERYU_REMOTE_PATH"
      expected="$(value --expected-head "$@")"
      release_tag_ref=
      release_tag_commit=
    else
      source_remote="$MOCK_SPLITOPS_REMOTE_PATH"
      expected="$(git --git-dir="$source_remote" rev-parse "$reference")"
      release_tag_ref="$(value --retain-exact-release-tag-ref "$@")"
      release_tag_commit="$(value --retain-exact-release-tag-commit "$@")"
      [[ "${MOCK_TAG_MISMATCH:-0}" != 1 ]]
      [[ "$(git --git-dir="$source_remote" rev-parse "$release_tag_ref")" \
        == "$release_tag_commit" ]]
    fi
    git clone -q --no-local --no-hardlinks "$source_remote" "$destination"
    git -C "$destination" checkout -q --detach "$expected"
    git -C "$destination" remote remove origin
    jq -n -c \
      --arg repository "$repo" --arg remote "$remote" \
      --arg reference "$reference" --arg commit "$expected" \
      --arg destination "$destination" --arg tag "$release_tag_ref" \
      --arg tag_commit "$release_tag_commit" \
      '{schema_version:"jain.jeryu-git-materialization/v1",
        repository:$repository,remote:$remote,reference:$reference,
        commit:$commit,destination:$destination,origin_retained:false,
        lfs_hydrated:false,release_tag_ref:$tag,
        release_tag_commit:$tag_commit,status:"pass"}'
    ;;
  *)
    exit 64
    ;;
esac
SPLITCTL
chmod 0500 "$splitctl"
splitctl_sha="$(sha256sum "$splitctl" | awk '{print $1}')"
jq -n -S \
  --arg install "$install" \
  --arg commit "$splitops_commit" \
  --arg tag "$splitops_tag" \
  --arg splitctl "$splitctl_sha" \
  --arg token "$token_file" \
  '{schema_version:"jain.native-build-tools-installer-config/v2",
    bootstrap_expires_at:"",
    install_dir:$install,
    control_remote:"http://127.0.0.1:8787/git/veox/jain-split-ops.git",
    control_ref:"refs/heads/main",control_commit:$commit,
    control_tag_ref:$tag,splitctl_sha256:$splitctl,token_file:$token}' \
  >"$splitops_config"
chmod 0600 "$splitops_config"
bootstrap_sha="$(sha256sum "$bootstrap" | awk '{print $1}')"
pin_sha="$(sha256sum "$pin_env" | awk '{print $1}')"
jq -n -S \
  --arg bootstrap "$bootstrap_sha" \
  --arg head "$head_sha" \
  --arg tree "$tree_sha" \
  --arg entrypoint "$bootstrap" \
  --arg predecessor "$predecessor_sha" \
  --arg pin "$pin_env" \
  --arg pin_sha "$pin_sha" \
  --arg splitctl "$splitctl" \
  --arg splitops_config "$splitops_config" \
  --arg state "$state" \
  --arg token "$token_file" \
  '{schema_version:"jeryu.jankurai-root-seal-authority/v1",
    bootstrap_sha256:$bootstrap,control_commit:$head,
    control_ref:"refs/heads/codex/jeryu-tool-jankurai-split3-production-digest-r17-20260802",
    control_remote:"http://127.0.0.1:8787/git/jeryu/jeryu-tool.git",
    control_tree:$tree,entrypoint_path:$entrypoint,
    expected_predecessor_sha256:$predecessor,pin_path:$pin,
    pin_sha256:$pin_sha,splitctl_path:$splitctl,
    splitops_config_path:$splitops_config,state_root:$state,
    token_file:$token}' >"$authority_config"
chmod 0600 "$authority_config"

receipt_stage="$evidence/receipt-stage.json"
jq -n -S \
  --arg remote "$JANKURAI_REPO" \
  --arg commit "$JANKURAI_REV" \
  --arg tag "$JANKURAI_TAG" \
  --arg source_tree "$JANKURAI_SOURCE_TREE" \
  --arg archive "$JANKURAI_SOURCE_ARCHIVE_SHA256" \
  --arg lock "$JANKURAI_CARGO_LOCK_SHA256" \
  --arg rustc "$JANKURAI_RUSTC_VERSION" \
  --arg cargo "$JANKURAI_CARGO_VERSION" \
  --arg triple "$JANKURAI_TARGET_TRIPLE" \
  --arg mode "$JANKURAI_BUILD_MODE" \
  --arg package_path "$JANKURAI_PACKAGE_PATH" \
  --arg builder_image "$JANKURAI_BUILDER_IMAGE" \
  --arg builder_image_id "$JANKURAI_BUILDER_IMAGE_ID" \
  --arg linker "$JANKURAI_LINKER_VERSION" \
  --arg glibc "$JANKURAI_GLIBC_VERSION" \
  --arg vendor "$JANKURAI_VENDOR_FILES_SHA256" \
  --arg vendor_count "$JANKURAI_VENDOR_FILE_COUNT" \
  --arg cargo_config "$JANKURAI_CARGO_CONFIG_SHA256" \
  --arg environment "$JANKURAI_BUILD_ENVIRONMENT" \
  --arg rustflags "$JANKURAI_RUSTFLAGS" \
  --arg command "$JANKURAI_BUILD_COMMAND" \
  --arg context "$JANKURAI_BUILD_CONTEXT_SHA256" \
  --arg digest "$candidate_sha" \
  --arg version "$JANKURAI_VERSION" \
  --arg path "$candidate" \
  --arg manifest_repo "http://127.0.0.1:8787/git/jeryu/jeryu-tool.git" \
  --arg manifest_commit "$head_sha" \
  --arg manifest_tree "$tree_sha" \
  --arg manifest_sha "$manifest_sha" \
  '{schema:"jeryu.jankurai-installation/v2",timestamp:"test",operator:"test",
    run_id:"test",test_mode:true,
    source:{remote:$remote,commit:$commit,tag:$tag,tree:$source_tree,
      archive_sha256:$archive,cargo_lock_sha256:$lock,
      verification:"diagnostic-candidate"},
    build:{rustc:$rustc,cargo:$cargo,target_triple:$triple,mode:$mode,
      package_path:$package_path,builder_image:$builder_image,
      builder_image_id:$builder_image_id,linker:$linker,glibc:$glibc,
      vendor_files_sha256:$vendor,vendor_file_count:$vendor_count,
      cargo_config_sha256:$cargo_config,environment:$environment,rustflags:$rustflags,
      command:$command,context_sha256:$context,cargo_net_offline:true,
      closed_vendor:true,network_none:true,read_only_root:true,non_root:true,
      capabilities_dropped:true,no_new_privileges:true,
      container_engine_path:"/usr/bin/docker",git_global_config_disabled:true,
      git_system_config_disabled:true,git_http_follow_redirects:false,
      git_terminal_prompt:false,jankurai_update_check:false,
      network_scope:"local-forge-source-plus-closed-vendor-network-none",
      no_proxy:"127.0.0.1,localhost,::1"},
    governance:{status:"diagnostic-candidate",manifest_repo:$manifest_repo,
      manifest_commit:$manifest_commit,manifest_tree:$manifest_tree,
      manifest_sha256:$manifest_sha,protected_main:false,
      protection_policy:"not-applicable"},
    binary:{sha256:$digest,version_output:$version},
    installation:{path:$path,atomic:true},conclusion:"success"}' \
  >"$receipt_stage"
receipt_sha="$(sha256sum "$receipt_stage" | awk '{print $1}')"
receipt="$evidence/$receipt_sha.json"
mv "$receipt_stage" "$receipt"

new_attempt() {
  printf '%s\n' "$(date +%s%N)-$RANDOM-$BASHPID" |
    sha256sum | awk '{print $1}'
}

make_request() {
  local output="$1" attempt="$2" filter="${3:-.}"
  jq -n -S \
    --arg attempt "$attempt" \
    --arg ref "$control_ref" \
    --arg head "$head_sha" \
    --arg tree "$tree_sha" \
    --arg candidate "$candidate" \
    --arg receipt "$receipt" \
    --arg receipt_sha "$receipt_sha" \
    '{schema:"jeryu.jankurai-root-seal-bootstrap/v1",attempt_id:$attempt,
      ref:$ref,head_sha:$head,tree_sha:$tree,candidate_path:$candidate,
      candidate_receipt_path:$receipt,
      candidate_receipt_sha256:$receipt_sha,
      created_at_epoch:990,expires_at_epoch:1100}' |
    jq "$filter" >"$output"
}

bootstrap_env=(
    JERYU_BOOTSTRAP_TEST_MODE=1 \
    JERYU_BOOTSTRAP_AUTHORITY_ROOT="$tmp" \
    JERYU_BOOTSTRAP_ENTRYPOINT="$bootstrap" \
    JERYU_BOOTSTRAP_AUTHORITY_CONFIG="$authority_config" \
    JERYU_BOOTSTRAP_SPLITOPS_CONFIG="$splitops_config" \
    JERYU_BOOTSTRAP_SPLITCTL="$splitctl" \
    JERYU_BOOTSTRAP_TOKEN_FILE="$token_file" \
    JERYU_BOOTSTRAP_REPO_ROOT="$repo" \
    JERYU_BOOTSTRAP_INSTALL_DIR="$install" \
    JERYU_BOOTSTRAP_STATE_ROOT="$state" \
    JERYU_BOOTSTRAP_REMOTE="http://127.0.0.1:8787/git/jeryu/jeryu-tool.git" \
    JERYU_BOOTSTRAP_PIN_ENV="$pin_env" \
    JERYU_BOOTSTRAP_TEST_ALLOW_HEAD_REUSE=1 \
    JERYU_BOOTSTRAP_NOW=1000 \
    MOCK_JERYU_REMOTE_PATH="$remote" \
    MOCK_SPLITOPS_REMOTE_PATH="$splitops_remote" \
    MOCK_HEAD="$head_sha" MOCK_CANDIDATE_SHA="$candidate_sha" \
    MOCK_SPLITOPS_COMMIT="$splitops_commit"
)
invoke() {
  env "${bootstrap_env[@]}" "$bootstrap" "$@"
}

assert_restored() {
  [[ "$(sha256sum "$install/jankurai" | awk '{print $1}')" == "$predecessor_sha" ]] ||
    fail 'protected predecessor binary was not restored'
  [[ "$(sha256sum "$publisher" | awk '{print $1}')" == "$publisher_original_sha" \
    && "$(sha256sum "$sandbox" | awk '{print $1}')" == "$sandbox_original_sha" ]] ||
    fail 'protected predecessor configs were not restored byte-for-byte'
  [[ ! -e "$state/active.json" ]] || fail 'active recovery marker survived restoration'
  if [[ -d "${state}-runner-custody" ]] \
    && find "${state}-runner-custody" -mindepth 1 -maxdepth 1 -print -quit |
      grep -q .; then
    fail 'root-held runner materialization survived restoration'
  fi
}

result_for_attempt() {
  local attempt="$1"
  local -a results
  mapfile -t results < <(find "$state/attempts/$attempt" -maxdepth 1 -type f \
    -regextype posix-extended -regex '.*/[0-9a-f]{64}\.json' -print)
  [[ "${#results[@]}" == 1 ]] ||
    fail "attempt $attempt did not publish exactly one content-addressed result"
  [[ "$(sha256sum "${results[0]}" | awk '{print $1}').json" == \
    "$(basename "${results[0]}")" ]] ||
    fail "attempt $attempt result content address is invalid"
  printf '%s\n' "${results[0]}"
}

request="$evidence/request-success.json"
success_attempt="$(new_attempt)"
make_request "$request" "$success_attempt"

expect_failure 'direct checkout bootstrap execution' \
  'production bootstrap must execute the installed entrypoint' \
  env "${bootstrap_env[@]}" "$source_bootstrap" "$request"

chmod 0777 "$install"
expect_failure 'writable installed ancestry' \
  'root-seal installed authority ancestry is not authority-owned and immutable' \
  invoke "$request"
chmod 0700 "$install"

chmod 0600 "$pin_env"
expect_failure 'writable installed pin helper' \
  'unsafe held installed Jankurai pin' invoke "$request"
chmod 0400 "$pin_env"

origin_url="$(git -C "$repo" config --local --get remote.origin.url)"
git -C "$repo" remote set-url origin \
  http://127.0.0.1:8787/git/jeryu/hostile-tool.git
expect_failure 'Jeryu Tool literal origin substitution' \
  'Jeryu Tool origin differs from bootstrap authority' invoke "$request"
git -C "$repo" remote set-url origin "$origin_url"
assert_restored

config_substitution_hostile() {
  local label="$1" target="$2" filter="$3" pattern="$4" hostile_request
  hostile_request="$evidence/request-${label}.json"
  make_request "$hostile_request" "$(new_attempt)"
  cp "$target" "$tmp/${label}.backup"
  jq "$filter" "$target" >"$tmp/${label}.hostile"
  mv -fT "$tmp/${label}.hostile" "$target"
  chmod 0600 "$target"
  expect_failure "$label" "$pattern" invoke "$hostile_request"
  mv -fT "$tmp/${label}.backup" "$target"
  chmod 0600 "$target"
  assert_restored
}
config_substitution_hostile jeryu-remote-substitution "$authority_config" \
  '.control_remote="http://127.0.0.1:8787/git/veox/hostile.git"' \
  'installed root-seal bootstrap authority config is invalid'
config_substitution_hostile jeryu-ref-substitution "$authority_config" \
  '.control_ref="refs/heads/codex/hostile"' \
  'installed root-seal bootstrap authority config is invalid'
config_substitution_hostile splitops-remote-substitution "$splitops_config" \
  '.control_remote="http://127.0.0.1:8787/git/veox/hostile.git"' \
  'installed SplitOps authority does not name the fixed forge'
config_substitution_hostile splitops-v1-downgrade "$splitops_config" \
  '.schema_version="jain.native-build-tools-installer-config/v1"' \
  'installed SplitOps authority config is invalid'
config_substitution_hostile splitops-missing-bootstrap-expiry "$splitops_config" \
  'del(.bootstrap_expires_at)' \
  'installed SplitOps authority config is invalid'
config_substitution_hostile splitops-unknown-field "$splitops_config" \
  '.unexpected_authority=true' \
  'installed SplitOps authority config is invalid'
config_substitution_hostile splitops-active-bootstrap-authority "$splitops_config" \
  '.bootstrap_expires_at="2099-01-01T00:00:00Z"' \
  'installed SplitOps authority config is invalid'
config_substitution_hostile splitops-expired-bootstrap-authority "$splitops_config" \
  '.bootstrap_expires_at="1970-01-01T00:00:00Z"' \
  'installed SplitOps authority config is invalid'
config_substitution_hostile splitops-commit-substitution "$splitops_config" \
  '.control_commit=("a" * 40)' \
  'cannot authenticate protected SplitOps main and immutable release tag'
config_substitution_hostile splitops-tag-substitution "$splitops_config" \
  '.control_tag_ref="refs/tags/jain-split-ops-v10.0.0-split.99"' \
  'cannot authenticate protected SplitOps main and immutable release tag'
config_substitution_hostile splitops-broker-digest-drift "$splitops_config" \
  '.splitctl_sha256=("a" * 64)' \
  'installed SplitOps broker/config binding is inconsistent'

protection_request="$evidence/request-protection-mismatch.json"
make_request "$protection_request" "$(new_attempt)"
expect_failure 'SplitOps protection mismatch' \
  'protected SplitOps main policy is not exact' \
  env "${bootstrap_env[@]}" MOCK_PROTECTION_MISMATCH=1 \
    "$bootstrap" "$protection_request"
assert_restored

caller_splitops="$tmp/caller-splitops"
git init -q "$caller_splitops"
git -C "$caller_splitops" config user.name 'Hostile Caller'
git -C "$caller_splitops" config user.email hostile@example.invalid
git -C "$caller_splitops" checkout -q -b hostile-head
printf 'hostile caller checkout\n' >"$caller_splitops/README"
git -C "$caller_splitops" add README
git -C "$caller_splitops" commit -q -m 'hostile caller head'
git -C "$caller_splitops" remote add origin "$tmp/hostile-origin.git"
poison_request="$evidence/request-caller-origin-head-poison.json"
poison_attempt="$(new_attempt)"
make_request "$poison_request" "$poison_attempt"
env "${bootstrap_env[@]}" \
  JERYU_BOOTSTRAP_TEST_CALLER_SPLITOPS_ROOT="$caller_splitops" \
  "$bootstrap" "$poison_request" >"$tmp/caller-poison.log"
assert_restored
result_for_attempt "$poison_attempt" >/dev/null

invoke "$request" >"$tmp/success.log"
grep -Fq 'predecessor_restored=true' "$tmp/success.log"
assert_restored
result="$(result_for_attempt "$success_attempt")"
jq -e --arg predecessor "$predecessor_sha" --arg candidate "$candidate_sha" '
  .conclusion == "success" and .seal_exit_code == 0
  and .predecessor_restored == true
  and .predecessor_sha256 == $predecessor
  and .candidate_sha256 == $candidate
  and (.runner_sha256 | test("^[0-9a-f]{64}$"))
  and (.parent_sha256 | test("^[0-9a-f]{64}$"))
  and (.integrity_sha256 | test("^[0-9a-f]{64}$"))
' "$result" >/dev/null

one_head_request="$evidence/request-one-head.json"
make_request "$one_head_request" "$(new_attempt)"
env "${bootstrap_env[@]}" JERYU_BOOTSTRAP_TEST_ALLOW_HEAD_REUSE=0 \
  "$bootstrap" "$one_head_request" >"$tmp/one-head-success.log"
assert_restored
second_head_request="$evidence/request-one-head-reuse.json"
make_request "$second_head_request" "$(new_attempt)"
expect_failure 'second attempt for exact head' \
  'exact head already consumed its sole root-seal attempt' \
  env "${bootstrap_env[@]}" JERYU_BOOTSTRAP_TEST_ALLOW_HEAD_REUSE=0 \
    "$bootstrap" "$second_head_request"

expect_failure 'attempt reuse' 'attempt identifier was already consumed' \
  invoke "$request"

bad="$evidence/request-extra-command.json"
make_request "$bad" "$(new_attempt)" '.command=["/bin/true"]'
expect_failure 'caller command injection' \
  'bootstrap request schema or closed fields are invalid' invoke "$bad"
expect_failure 'extra positional command' 'usage:' invoke "$request" /bin/true

bad_candidate="$evidence/jankurai-wrong"
printf '#!/usr/bin/env bash\nprintf "jankurai test-candidate\\n"\n#wrong\n' \
  >"$bad_candidate"
chmod 0755 "$bad_candidate"
bad="$evidence/request-wrong-digest.json"
make_request "$bad" "$(new_attempt)"
jq --arg path "$bad_candidate" '.candidate_path=$path' "$bad" >"$bad.next"
mv "$bad.next" "$bad"
expect_failure 'wrong candidate digest' \
  'qualified candidate digest differs from the reviewed pin' invoke "$bad"

bad="$evidence/request-wrong-ref.json"
make_request "$bad" "$(new_attempt)" '.ref="refs/heads/codex/wrong-ref"'
expect_failure 'wrong ref' 'bootstrap request differs from the installed reviewed authority' \
  invoke "$bad"
bad="$evidence/request-wrong-head.json"
make_request "$bad" "$(new_attempt)" '.head_sha=("a"*40)'
expect_failure 'wrong head' 'bootstrap request differs from the installed reviewed authority' \
  invoke "$bad"
bad="$evidence/request-wrong-tree.json"
make_request "$bad" "$(new_attempt)" '.tree_sha=("b"*40)'
expect_failure 'wrong tree' 'bootstrap request differs from the installed reviewed authority' \
  invoke "$bad"
bad="$evidence/request-expired.json"
make_request "$bad" "$(new_attempt)" \
  '.created_at_epoch=800 | .expires_at_epoch=900'
expect_failure 'expired request' 'bootstrap request is expired' invoke "$bad"
bad="$evidence/request-long-lease.json"
make_request "$bad" "$(new_attempt)" \
  '.created_at_epoch=100 | .expires_at_epoch=1100'
expect_failure 'overlong request' 'bootstrap request is expired' invoke "$bad"

tampered_receipt_stage="$evidence/tampered-receipt-stage.json"
jq '.governance.protected_main=true' "$receipt" >"$tampered_receipt_stage"
tampered_receipt_sha="$(sha256sum "$tampered_receipt_stage" | awk '{print $1}')"
tampered_receipt="$evidence/$tampered_receipt_sha.json"
mv "$tampered_receipt_stage" "$tampered_receipt"
bad="$evidence/request-tampered-receipt.json"
make_request "$bad" "$(new_attempt)"
jq --arg path "$tampered_receipt" --arg sha "$tampered_receipt_sha" \
  '.candidate_receipt_path=$path | .candidate_receipt_sha256=$sha' \
  "$bad" >"$bad.next"
mv "$bad.next" "$bad"
expect_failure 'tampered receipt' \
  'candidate qualification receipt does not bind the exact candidate' invoke "$bad"

failed_attempt="$(new_attempt)"
bad="$evidence/request-failed-command.json"
make_request "$bad" "$failed_attempt"
# Invoke directly so the expected nonzero result remains inspectable.
set +e
MOCK_EXIT_CODE=23 invoke "$bad" >"$tmp/failed-command.log" 2>&1
failed_rc=$?
set -e
[[ "$failed_rc" == 23 ]] || fail 'failed seal command exit code was not preserved'
assert_restored
jq -e '.conclusion == "failure" and .seal_exit_code == 23
  and .predecessor_restored == true' \
  "$(result_for_attempt "$failed_attempt")" >/dev/null

replacement_request="$evidence/request-candidate-replacement.json"
make_request "$replacement_request" "$(new_attempt)"
ready="$tmp/replacement.ready"
release="$tmp/replacement.release"
cp "$candidate" "$tmp/candidate.backup"
env "${bootstrap_env[@]}" \
  JERYU_BOOTSTRAP_TEST_PAUSE_READY_FILE="$ready" \
  JERYU_BOOTSTRAP_TEST_PAUSE_RELEASE_FILE="$release" \
  "$bootstrap" "$replacement_request" >"$tmp/candidate-replacement.log" 2>&1 &
replacement_pid=$!
while [[ ! -e "$ready" ]]; do read -r -t 0.05 _ </dev/null || true; done
printf '#!/usr/bin/env bash\nexit 0\n' >"$tmp/candidate.hostile"
chmod 0755 "$tmp/candidate.hostile"
mv -fT "$tmp/candidate.hostile" "$candidate"
: >"$release"
if wait "$replacement_pid"; then
  fail 'candidate replacement hostile unexpectedly succeeded'
fi
grep -Fq 'qualified candidate replacement detected' \
  "$tmp/candidate-replacement.log"
mv -fT "$tmp/candidate.backup" "$candidate"
chmod 0755 "$candidate"
assert_restored

replacement_request="$evidence/request-candidate-content-drift.json"
make_request "$replacement_request" "$(new_attempt)"
rm -f "$ready" "$release"
cp "$candidate" "$tmp/candidate.backup"
env "${bootstrap_env[@]}" \
  JERYU_BOOTSTRAP_TEST_PAUSE_READY_FILE="$ready" \
  JERYU_BOOTSTRAP_TEST_PAUSE_RELEASE_FILE="$release" \
  "$bootstrap" "$replacement_request" >"$tmp/candidate-content-drift.log" 2>&1 &
replacement_pid=$!
while [[ ! -e "$ready" ]]; do read -r -t 0.05 _ </dev/null || true; done
printf '\n# same-inode candidate content drift\n' >>"$candidate"
: >"$release"
if wait "$replacement_pid"; then
  fail 'candidate same-inode content drift hostile unexpectedly succeeded'
fi
grep -Fq 'qualified candidate content drift detected' \
  "$tmp/candidate-content-drift.log"
cp "$tmp/candidate.backup" "$candidate"
rm -f "$tmp/candidate.backup"
chmod 0755 "$candidate"
assert_restored

replacement_request="$evidence/request-broker-replacement.json"
make_request "$replacement_request" "$(new_attempt)"
rm -f "$ready" "$release"
env "${bootstrap_env[@]}" \
  JERYU_BOOTSTRAP_TEST_PAUSE_READY_FILE="$ready" \
  JERYU_BOOTSTRAP_TEST_PAUSE_RELEASE_FILE="$release" \
  "$bootstrap" "$replacement_request" >"$tmp/replacement.log" 2>&1 &
replacement_pid=$!
while [[ ! -e "$ready" ]]; do read -r -t 0.05 _ </dev/null || true; done
printf 'hostile broker replacement\n' >"$install/jankurai.hostile"
chmod 0555 "$install/jankurai.hostile"
mv -fT "$install/jankurai.hostile" "$install/jankurai"
: >"$release"
if wait "$replacement_pid"; then
  fail 'broker replacement hostile unexpectedly succeeded'
fi
grep -Fq 'broker or config replacement detected' "$tmp/replacement.log"
assert_restored

replacement_request="$evidence/request-config-replacement.json"
make_request "$replacement_request" "$(new_attempt)"
rm -f "$ready" "$release"
env "${bootstrap_env[@]}" \
  JERYU_BOOTSTRAP_TEST_PAUSE_READY_FILE="$ready" \
  JERYU_BOOTSTRAP_TEST_PAUSE_RELEASE_FILE="$release" \
  "$bootstrap" "$replacement_request" >"$tmp/config-replacement.log" 2>&1 &
replacement_pid=$!
while [[ ! -e "$ready" ]]; do read -r -t 0.05 _ </dev/null || true; done
printf '{"hostile":true}\n' >"$install/publisher.hostile"
chmod 0600 "$install/publisher.hostile"
mv -fT "$install/publisher.hostile" "$publisher"
: >"$release"
if wait "$replacement_pid"; then
  fail 'config replacement hostile unexpectedly succeeded'
fi
grep -Fq 'broker or config replacement detected' "$tmp/config-replacement.log"
assert_restored

live_authority_hostile() {
  local label="$1" target="$2" mutation="$3" mode request_path
  request_path="$evidence/request-${label}.json"
  make_request "$request_path" "$(new_attempt)"
  ready="$tmp/${label}.ready"
  release="$tmp/${label}.release"
  mode="$(stat -Lc '%a' -- "$target")"
  cp "$target" "$tmp/${label}.backup"
  env "${bootstrap_env[@]}" \
    JERYU_BOOTSTRAP_TEST_PAUSE_READY_FILE="$ready" \
    JERYU_BOOTSTRAP_TEST_PAUSE_RELEASE_FILE="$release" \
    "$bootstrap" "$request_path" >"$tmp/${label}.log" 2>&1 &
  replacement_pid=$!
  while [[ ! -e "$ready" ]]; do read -r -t 0.05 _ </dev/null || true; done
  if [[ "$mutation" == replace ]]; then
    printf '#!/usr/bin/env bash\nexit 0\n' >"$tmp/${label}.hostile"
    chmod "$mode" "$tmp/${label}.hostile"
    mv -fT "$tmp/${label}.hostile" "$target"
  else
    chmod u+w "$target"
    printf '\n# same-inode installed authority drift\n' >>"$target"
  fi
  : >"$release"
  if wait "$replacement_pid"; then
    fail "$label hostile unexpectedly succeeded"
  fi
  grep -Eq 'unsafe held|installed root-seal authority identity or content drift detected' \
    "$tmp/${label}.log"
  mv -fT "$tmp/${label}.backup" "$target"
  chmod "$mode" "$target"
  assert_restored
}

live_authority_hostile installed-splitctl-replacement "$splitctl" replace
live_authority_hostile installed-splitctl-same-inode "$splitctl" drift
live_authority_hostile installed-config-replacement "$authority_config" replace
live_authority_hostile installed-entrypoint-same-inode "$bootstrap" drift

interrupt_attempt="$(new_attempt)"
interrupt_request="$evidence/request-interrupt.json"
make_request "$interrupt_request" "$interrupt_attempt"
interrupt_ready="$tmp/interrupt.ready"
env "${bootstrap_env[@]}" MOCK_READY_FILE="$interrupt_ready" MOCK_SLEEP_SECONDS=30 \
  "$bootstrap" "$interrupt_request" >"$tmp/interrupt.log" 2>&1 &
interrupt_pid=$!
while [[ ! -e "$interrupt_ready" ]]; do read -r -t 0.05 _ </dev/null || true; done
kill -TERM "$interrupt_pid"
if wait "$interrupt_pid"; then
  fail 'interrupted transaction unexpectedly succeeded'
fi
assert_restored
grep -Fq 'exit-trap-restoration' \
  "$state/attempts/$interrupt_attempt/restoration-status"

kill_attempt="$(new_attempt)"
kill_request="$evidence/request-kill.json"
make_request "$kill_request" "$kill_attempt"
kill_ready="$tmp/kill.ready"
env "${bootstrap_env[@]}" MOCK_READY_FILE="$kill_ready" MOCK_SLEEP_SECONDS=30 \
  "$bootstrap" "$kill_request" >"$tmp/kill.log" 2>&1 &
kill_pid=$!
while [[ ! -e "$kill_ready" ]]; do read -r -t 0.05 _ </dev/null || true; done
kill -KILL "$kill_pid"
wait "$kill_pid" 2>/dev/null || true
[[ -e "$state/active.json" ]] ||
  fail 'kill hostile did not leave the durable recovery marker'
[[ "$(sha256sum "$install/jankurai" | awk '{print $1}')" == "$candidate_sha" ]] ||
  fail 'kill hostile did not exercise candidate-live recovery'
recovery_request="$evidence/request-recovery.json"
make_request "$recovery_request" "$(new_attempt)"
invoke "$recovery_request" >"$tmp/recovery.log"
assert_restored
grep -Fq 'interrupted-recovery' "$state/attempts/$kill_attempt/restoration-status"

grep -Fq '/usr/local/libexec/jain' "$bootstrap" ||
  fail 'fixed production broker install root is absent'
grep -Fq 'production bootstrap is root-only' "$bootstrap" ||
  fail 'root-only production authority is absent'
grep -Fq 'production override is forbidden' "$bootstrap" ||
  fail 'caller broker/command overrides are not rejected'
grep -Fq 'protected SplitOps main and immutable tag trees differ' "$bootstrap" ||
  fail 'protected runner authority validation is absent'
grep -Fq 'protected_main' "${here}/install-jankurai.sh" ||
  fail 'production installer protected-main validation is absent'
grep -Fq 'release broker Jankurai rejects caller receipt authority' \
  "${here}/ci/lib.sh" ||
  fail 'ordinary release broker still accepts caller receipt authority'

printf 'root-seal bootstrap tests passed: installed-entrypoint immutable-blobs fixed-forge protection tag digest ref head tree receipt expiry reuse command replacement same-inode-drift held-execution interruption recovery restoration\n'
