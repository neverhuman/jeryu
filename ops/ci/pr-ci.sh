#!/usr/bin/env bash
# Comprehensive local PR gate. host-ci runs this to produce the forge `jeryu/ci`
# check-run, which is THE gate for forge PRs (the GitHub-Actions workflow lanes run
# on the GitHub mirror's real runners; the forge does not seed them — see
# crates/jeryu-api/src/ci_bridge.rs). This therefore carries the real equivalent
# coverage locally: format + clippy (deny warnings) + the FULL workspace test suite
# + the jankurai audit (>= 85). The security/web lanes (syft/grype/cosign/playwright)
# run on the mirror; invoke them here too once their tooling is provisioned on the
# runner.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

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
JANKURAI_BIN="${JANKURAI_BIN:-$HOME/.cargo/bin/jankurai}"

echo "[pr-ci] (jobs=$JOBS) cargo fmt --all --check" >&2
cargo fmt --all --check

echo "[pr-ci] cargo clippy --workspace --all-targets -- -D warnings" >&2
cargo clippy --workspace --all-targets --jobs "$JOBS" -- -D warnings

# The kernel-sandbox-runtime integration tests spawn REAL sandboxes (user/mount/pid
# namespaces + cgroup-v2 + landlock/seccomp). They require an UNMANAGED cgroup
# environment and fail under host-ci's systemd-managed poll cgroup
# (cgroup_create EEXIST / clone EOPNOTSUPP). They run on the dedicated GitHub-mirror
# runners (full caps). Exclude exactly those here; the other 1600+ tests still run.
echo "[pr-ci] cargo test (excl. jeryu-sandbox-linux + agentbridge sandbox-runtime tests)" >&2
cargo test --workspace --exclude jeryu-sandbox-linux --jobs "$JOBS" --no-fail-fast -- \
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
  --skip terminate_stops_a_runaway_agent

echo "[pr-ci] jankurai audit (>= 85)" >&2
"$JANKURAI_BIN" . --json .jankurai/repo-score.json --md .jankurai/repo-score.md
python3 - <<'PY'
import json, sys
d = json.load(open(".jankurai/repo-score.json"))
score = d.get("score", 0)
caps = d.get("caps_applied", [])
print(f"[pr-ci] jankurai score={score} caps={caps}", file=sys.stderr)
sys.exit(0 if score >= 85 and not caps else 1)
PY

echo "[pr-ci] PASS — fmt + clippy + workspace tests + jankurai all green" >&2
