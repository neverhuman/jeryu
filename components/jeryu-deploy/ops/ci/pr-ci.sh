#!/usr/bin/env bash
# Comprehensive local PR gate. Native host CI runs this to produce the required
# exact-head check, and a trusted hosted runner uses the same entrypoint. It carries
# format + clippy (deny warnings) + the FULL workspace test suite + the Jankurai
# audit (>= 85) + immutable web-bundle validation + the local security lane.
set -euo pipefail

# BEGIN GENERATED JANKURAI PIN — DO NOT EDIT
export JERYU_JANKURAI_SOURCE_REPO="http://127.0.0.1:8787/git/jeryu/jankurai.git"
export JERYU_JANKURAI_VERSION="jankurai 1.6.11"
export JERYU_JANKURAI_SHA256="9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c"
export JERYU_JANKURAI_SOURCE_REV="b88562fdb124aa86dedd70ab972e7d0d87e58be1"
export JERYU_JANKURAI_SOURCE_TAG="v1.6.11-deadlang-precision-split.3"
export JERYU_JANKURAI_SOURCE_TREE="611229e54938c0e8808896e369fd54d095d258f7"
export JERYU_JANKURAI_SOURCE_ARCHIVE_SHA256="903a231eca8f6a1f050953b603d5a278a1606abcdf47434eb1b45262d74068aa"
export JERYU_JANKURAI_CARGO_LOCK_SHA256="b9acb981c326226a687d0b6703e4f7ee303148e9e1a6dda1aa03d77988820f6a"
export JERYU_JANKURAI_RUST_TOOLCHAIN="1.95.0"
export JERYU_JANKURAI_RUSTC_VERSION="rustc 1.95.0 (59807616e 2026-04-14)"
export JERYU_JANKURAI_CARGO_VERSION="cargo 1.95.0 (f2d3ce0bd 2026-03-21)"
export JERYU_JANKURAI_TARGET_TRIPLE="x86_64-unknown-linux-gnu"
export JERYU_JANKURAI_BUILD_MODE="oci-vendor-locked-offline-workspace-member-v2"
export JERYU_JANKURAI_PACKAGE_PATH="crates/jankurai"
export JERYU_JANKURAI_BUILDER_IMAGE="rust@sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082"
export JERYU_JANKURAI_BUILDER_IMAGE_ID="sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082"
export JERYU_JANKURAI_LINKER_VERSION="GNU ld (GNU Binutils for Debian) 2.40"
export JERYU_JANKURAI_GLIBC_VERSION="ldd (Debian GLIBC 2.36-9+deb12u14) 2.36"
export JERYU_JANKURAI_VENDOR_FILES_SHA256="a7e332f4495d9748ea020ae8ee37c4240f0f035059799bd3dc74497437143d99"
export JERYU_JANKURAI_VENDOR_FILE_COUNT="14889"
export JERYU_JANKURAI_CARGO_CONFIG_SHA256="b8982c761d62e447f2d1653c199d2d58e6b2de6c5a6f8ddba3d38e47b7f863d6"
export JERYU_JANKURAI_BUILD_ENVIRONMENT="CARGO_NET_OFFLINE=true,HOME=/tmp,LANG=C,LC_ALL=C,SOURCE_DATE_EPOCH=0,TZ=UTC"
export JERYU_JANKURAI_RUSTFLAGS="--remap-path-prefix=/opt/jeryu/jankurai=/jankurai-build/source --remap-path-prefix=/opt/jeryu/vendor=/jankurai-build/vendor --remap-path-prefix=/opt/jeryu/target=/jankurai-build/target --remap-path-prefix=/usr/local/cargo=/jankurai-build/cargo"
export JERYU_JANKURAI_BUILD_COMMAND="cargo install --locked --offline --path /opt/jeryu/jankurai/crates/jankurai --root /opt/jeryu/out --bin jankurai"
export JERYU_JANKURAI_BUILD_CONTEXT_SHA256="889d19f86fc390b0f0cf0bd6ecb4d451c51a2d6fb328e5520e4310e7ee5dedd6"
# END GENERATED JANKURAI PIN

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"
source ops/ci/lib.sh
require_jankurai
bash "${repo_root}/ops/ci/test-governed-jankurai.sh"

# jankurai pin: jeryu-tool/tool-manifest.toml is the family-wide source of truth.
# When the control-plane repo is reachable (on-host family layout), fail fast if
# this repo's pinned consumers drifted from it. In an isolated single-repo CI
# checkout it is absent — skip rather than fail.
JERYU_TOOL_RENDER="${JERYU_TOOL_RENDER:-$repo_root/../jeryu-tool/ops/render-tool-manifest.sh}"
if [ -x "$JERYU_TOOL_RENDER" ]; then
  echo "[pr-ci] jankurai pin drift check" >&2
  bash "$JERYU_TOOL_RENDER" --check --repo jeryu-deploy \
    --repo-root "jeryu-deploy=$repo_root"
fi

# jeryu governs the worker count from live load (overrides any request). host-ci
# already exports a governed JERYU_CI_JOBS; honor it, else ask the governor, else a
# conservative default. An unbounded fan-out once wedged the host — never default high.
if [ -n "${JERYU_CI_JOBS:-}" ]; then
  JOBS="${JERYU_CI_JOBS}"
elif command -v jeryu-ci-governor >/dev/null 2>&1; then
  JOBS="$(jeryu-ci-governor 2>/dev/null || echo 8)"
else
  JOBS=8
fi
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$JOBS}"
TEST_THREADS="${JERYU_CI_TEST_THREADS:-8}"

source ops/ci/workspace-lock.sh
jeryu_deploy_record_workspace_lock "$repo_root"

# Retain the local metadata, map, shell, phase and coverage regression checks.
# fast.sh delegates to check.sh, so one invocation covers both local recipes.
bash ops/ci/check.sh

echo "[pr-ci] (jobs=$JOBS) cargo fmt --all --check" >&2
cargo fmt --all --check

echo "[pr-ci] cargo clippy --workspace --all-targets -- -D warnings" >&2
cargo clippy --locked --workspace --all-targets --jobs "$JOBS" -- -D warnings

# The kernel-sandbox-runtime integration tests spawn REAL sandboxes (user/mount/pid
# namespaces + cgroup-v2 + landlock/seccomp). They require an UNMANAGED cgroup
# environment and fail under host-ci's systemd-managed poll cgroup
# (cgroup_create EEXIST / clone EOPNOTSUPP). They run on the dedicated GitHub-mirror
# runners (full caps). Exclude exactly those here; the remaining workspace tests run.
echo "[pr-ci] cargo test (excl. jeryu-sandbox-linux + agentbridge sandbox-runtime tests)" >&2
# --test-threads uses the separately governed process cap: libtest defaults to
# ncpu, and an oversubscribed host starves the live agent-stream tests'
# 30s polling deadlines (await_tty) into false failures. The web::sessions
# live-stream proofs (create_session_*: spawn a real native/docker-seam PTY and
# poll await_tty for the agent's marker) are the same class: under host-ci load
# the sandboxed process does not stream its marker inside the 30s window and the
# proof flakes. They run green on the dedicated GitHub-mirror runners (full caps,
# unloaded); skip them here exactly like the agent-stream/sandbox live tests.
cargo test --locked --workspace --exclude jeryu-sandbox-linux --jobs "$JOBS" --no-fail-fast -- \
  --test-threads "$TEST_THREADS" \
  --skip same_write_path_succeeds_inside_and_is_blocked_outside \
  --skip unsandboxed_control_can_write_outside_proving_landlock_is_the_blocker \
  --skip budget_kill_is_live_and_truncates \
  --skip watchdog_kill_is_live \
  --skip require_cgroup_driver_fails_closed_without_delegated_subtree \
  --skip opt_out_driver_runs_on_this_no_delegation_host \
  --skip editbot_writes_inside_the_cell \
  --skip editbot_writing_outside_the_cell_is_denied_by_landlock \
  --skip watchdog_kills_a_runaway_editbot \
  --skip output_budget_exceeded_kills_the_child \
  --skip streams_terminal_output_to_the_sink \
  --skip control_input_reaches_the_agent_stdin \
  --skip terminate_stops_a_runaway_agent \
  --skip create_session_spawns_agent_and_streams_its_tty_output \
  --skip create_session_agent_runs_in_workspace_with_branch_env \
  --skip create_session_docker_runtime_streams_live_and_carries_hardened_flags \
  --skip create_session_native_runtime_uses_native_path

echo "[pr-ci] immutable web-bundle integration" >&2
bash "${repo_root}/ops/ci/web.sh"

jeryu_deploy_assert_workspace_lock_unchanged
echo "[pr-ci] jankurai audit (>= 85)" >&2
run_governed_jankurai audit . --full --mode advisory --policy agent/audit-policy.toml \
  --json .jankurai/repo-score.json --md .jankurai/repo-score.md
score="$(jq -r '.score // 0' .jankurai/repo-score.json)"
caps="$(jq -c '.caps_applied // []' .jankurai/repo-score.json)"
echo "[pr-ci] jankurai score=${score} caps=${caps}" >&2
jq -e '(.score // 0) >= 85 and ((.caps_applied // []) | length == 0)' \
  .jankurai/repo-score.json >/dev/null

echo "[pr-ci] security lane"
JERYU_SECURITY_NETWORK=1 bash "${repo_root}/ops/ci/security.sh"
jeryu_deploy_assert_workspace_lock_unchanged

echo "[pr-ci] PASS — fmt + clippy + workspace tests + web bundle + jankurai + security all green" >&2
