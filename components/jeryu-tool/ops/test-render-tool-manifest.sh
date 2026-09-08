#!/usr/bin/env bash
# Custody and fail-closed tests for render-tool-manifest.sh.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/.." && pwd)"
renderer="${here}/render-tool-manifest.sh"
tmp="$(mktemp -d /tmp/test-render-tool-manifest.XXXXXX)"
trap 'rm -rf "${tmp}"' EXIT

fail() {
  printf 'test-render-tool-manifest: %s\n' "$*" >&2
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

# Rust unit tests prove rendering is keyed and shape-based against a fabricated
# future predecessor. Build the exact binary once for the custody hostiles.
cargo build --quiet --locked --offline --manifest-path "${repo_root}/Cargo.toml" \
  --bin jeryu-toolctl
toolctl="${repo_root}/target/debug/jeryu-toolctl"

init_repo() {
  local root="$1" origin="$2"
  git init -q "${root}"
  git -C "${root}" config user.name renderer-test
  git -C "${root}" config user.email renderer-test@localhost
  git -C "${root}" remote add origin "${origin}"
  mkdir -p "${root}/ops/ci"
  printf '#!/usr/bin/env bash\nset -euo pipefail\n' > "${root}/ops/ci/lib.sh"
  git -C "${root}" add ops/ci/lib.sh
  git -C "${root}" commit -q -m baseline
  git -C "${root}" branch -M main
  git -C "${root}" update-ref refs/remotes/origin/main HEAD
}

canonical_fixture="${tmp}/canonical-fixture"
init_repo "${canonical_fixture}" \
  "https://git.neverhuman.org/git/jeryu/jeryu.git"

# No-argument mode detects family drift but never writes it.
family="${tmp}/family"
mkdir -p "${family}/jeryu/ops/ci"
printf '#!/usr/bin/env bash\nset -euo pipefail\n' > "${family}/jeryu/ops/ci/lib.sh"
before="$(sha256sum "${family}/jeryu/ops/ci/lib.sh" | awk '{print $1}')"
expect_failure "unscoped check-only" "unscoped renderer invocation is check-only" \
  bash "${renderer}" --family-root "${family}"
after="$(sha256sum "${family}/jeryu/ops/ci/lib.sh" | awk '{print $1}')"
[[ "${before}" == "${after}" ]] || fail "unscoped invocation mutated a consumer"

# Bundle-restored and other standalone no-local clones may run every read-only
# test, but their only permitted write-mode outcome is refusal at tool-source
# custody. The canonical checkout below exercises the remaining write hostiles.
canonical_tool_root="/home/ubuntu/jain-split/jeryu-split/jeryu-tool"
if [[ "${repo_root}" != "${canonical_tool_root}" ]]; then
  repo_head="$(git -C "${repo_root}" rev-parse HEAD)"
  expect_failure "standalone renderer source" "exact canonical family root" \
    bash "${renderer}" --repo jeryu-tool \
      --repo-root "jeryu-tool=${repo_root}" \
      --expected-head "jeryu-tool=${repo_head}"
  printf 'render-tool-manifest tests passed: read-only clone refused write custody\n'
  exit 0
fi

expect_failure "missing explicit root" "requires an explicit --repo-root" \
  bash "${renderer}" --repo jeryu
expect_failure "duplicate repository selector" "duplicate --repo" \
  bash "${renderer}" --check --repo jeryu --repo jeryu
expect_failure "duplicate family root" "duplicate --family-root" \
  bash "${renderer}" --check --family-root "${family}" --family-root "${family}"

canonical="https://git.neverhuman.org/git/jeryu/jeryu.git"
dirty="${tmp}/dirty"
init_repo "${dirty}" "${canonical}"
dirty_head="$(git -C "${dirty}" rev-parse HEAD)"
printf '# dirty\n' >> "${dirty}/ops/ci/lib.sh"
expect_failure "noncanonical dirty root" "not the canonical physical checkout" \
  bash "${renderer}" --repo jeryu --repo-root "jeryu=${dirty}" \
    --expected-head "jeryu=${dirty_head}"

wrong_origin="${tmp}/wrong-origin"
init_repo "${wrong_origin}" "http://example.invalid/jeryu.git"
wrong_origin_head="$(git -C "${wrong_origin}" rev-parse HEAD)"
expect_failure "noncanonical wrong-origin root" "not the canonical physical checkout" \
  bash "${renderer}" --repo jeryu --repo-root "jeryu=${wrong_origin}" \
    --expected-head "jeryu=${wrong_origin_head}"

unrelated="${tmp}/unrelated"
init_repo "${unrelated}" "${canonical}"
unrelated_head="$(git -C "${unrelated}" rev-parse HEAD)"
expect_failure "noncanonical alternate repository" "not the canonical physical checkout" \
  bash "${renderer}" --repo jeryu --repo-root "jeryu=${unrelated}" \
    --expected-head "jeryu=${unrelated_head}"

repo_head="$(git -C "${repo_root}" rev-parse HEAD)"
expect_failure "missing expected head" "requires --expected-head" \
  bash "${renderer}" --repo jeryu-tool --repo-root "jeryu-tool=${repo_root}"
expect_failure "malformed expected head" "invalid --expected-head" \
  bash "${renderer}" --repo jeryu-tool --repo-root "jeryu-tool=${repo_root}" \
    --expected-head "jeryu-tool=not-a-sha"
expect_failure "duplicate expected head" "duplicate --expected-head" \
  bash "${renderer}" --repo jeryu-tool --repo-root "jeryu-tool=${repo_root}" \
    --expected-head "jeryu-tool=${repo_head}" --expected-head "jeryu-tool=${repo_head}"
expect_failure "unselected expected head" "without matching --repo" \
  bash "${renderer}" --repo jeryu-tool --repo-root "jeryu-tool=${repo_root}" \
    --expected-head "jeryu=${repo_head}"

# A clean canonical linear descendant is still the wrong worktree when its
# handed-off SHA names the protected-main parent. Refuse before touching bytes.
wrong_descendant="${tmp}/wrong-descendant"
git clone -q --no-local "${canonical_fixture}" "${wrong_descendant}"
git -C "${wrong_descendant}" remote set-url origin "${canonical}"
git -C "${wrong_descendant}" config user.name renderer-test
git -C "${wrong_descendant}" config user.email renderer-test@localhost
printf 'unrelated descendant\n' > "${wrong_descendant}/wrong-descendant.txt"
git -C "${wrong_descendant}" add wrong-descendant.txt
git -C "${wrong_descendant}" commit -q -m 'unrelated descendant'
handed_off_head="$(git -C "${wrong_descendant}" rev-parse HEAD^)"
descendant_before="$(sha256sum "${wrong_descendant}/ops/ci/lib.sh" | awk '{print $1}')"
expect_failure "alternate clean descendant" "not the canonical physical checkout" \
  bash "${renderer}" --repo jeryu --repo-root "jeryu=${wrong_descendant}" \
    --expected-head "jeryu=${handed_off_head}"
descendant_after="$(sha256sum "${wrong_descendant}/ops/ci/lib.sh" | awk '{print $1}')"
[[ "${descendant_before}" == "${descendant_after}" ]] ||
  fail "HEAD mismatch mutated the consumer before refusal"

# A consumer-only write cannot repair or otherwise touch the renderer owner's
# generated env as an implicit side effect.
renderer_fixture="${tmp}/renderer-owner"
mkdir -p "${renderer_fixture}/ops/render-assets" "${renderer_fixture}/generated"
cp "${here}/render-assets/require-jankurai.sh" \
  "${renderer_fixture}/ops/render-assets/"
cp "${repo_root}/tool-manifest.toml" "${renderer_fixture}/"
printf 'deliberately stale owner pin\n' > "${renderer_fixture}/generated/jankurai-pin.env"
scoped_consumer="${tmp}/scoped-consumer"
git clone -q --no-local "${canonical_fixture}" "${scoped_consumer}"
git -C "${scoped_consumer}" remote set-url origin "${canonical}"
scoped_head="$(git -C "${scoped_consumer}" rev-parse HEAD)"
expect_failure "alternate renderer source" "exact canonical family root" \
  "${toolctl}" --tool-root "${renderer_fixture}" render-tool-manifest --repo jeryu \
    --repo-root "jeryu=${scoped_consumer}" \
    --expected-head "jeryu=${scoped_head}"
[[ "$(cat "${renderer_fixture}/generated/jankurai-pin.env")" == \
  "deliberately stale owner pin" ]] || fail "consumer render mutated manifest-owner pin"

# Standalone no-local clones remain valid only as read-only drift fixtures.
# They can never become renderer write roots, even with the canonical remote and
# exact handed-off head.
manifest_owner="${tmp}/manifest-owner"
git clone -q --no-local "${repo_root}" "${manifest_owner}"
git -C "${manifest_owner}" remote set-url origin \
  "https://git.neverhuman.org/git/jeryu/jeryu-tool.git"
expect_failure "alternate manifest-owner clone" "not the canonical physical checkout" \
  bash "${renderer}" --repo jeryu-tool --repo-root "jeryu-tool=${manifest_owner}" \
    --expected-head "jeryu-tool=${repo_head}"
[[ -z "$(git -C "${manifest_owner}" status --porcelain --untracked-files=all)" ]] ||
  fail "rejected alternate clone was dirtied"

clone_before="$(sha256sum "${manifest_owner}/generated/jankurai-pin.env" | awk '{print $1}')"
bash "${renderer}" --check --repo jeryu-tool \
  --repo-root "jeryu-tool=${manifest_owner}" >/dev/null
clone_after="$(sha256sum "${manifest_owner}/generated/jankurai-pin.env" | awk '{print $1}')"
[[ "${clone_before}" == "${clone_after}" ]] ||
  fail "read-only clone check mutated generated bytes"

printf 'render-tool-manifest tests passed: canonical-only writes token-safe hosted checks closed scope\n'
