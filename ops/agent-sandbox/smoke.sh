#!/usr/bin/env bash
# Real-engine smoke for the confined agent-sandbox image.
#
# It BUILDS the image and runs the hardened container with the EXACT lock-down flags
# `OciSpec::from_agent_job` (crates/jeryu-runner-oci) emits, then PROVES the lockdown
# actually holds on a live engine: read-only root, --network none, the refusal wrappers,
# the seccomp symlink block, the git guard, and the in-image toolchain. Each property is
# one container run with a clear pass/fail line.
#
# Usage:  ops/agent-sandbox/smoke.sh [smoke|full]
# Engine: ${JERYU_OCI_RUNTIME:-podman} (matches the runner's runtime selection).
#
# When no container engine is present the script prints a clear SKIP line and exits 0, so
# it is safe to invoke from any lane; the assertions only run where an engine exists (a
# dedicated runner). The daemonless CI never reaches the engine path.
set -euo pipefail

mode="${1:-smoke}"
runtime="${JERYU_OCI_RUNTIME:-podman}"
image="localhost/jeryu/agent-sandbox:smoke"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if ! command -v "$runtime" >/dev/null 2>&1; then
  echo "agent-sandbox smoke: SKIP (no container runtime: '$runtime' not on PATH)"
  exit 0
fi

# The seccomp profile the engine reads from the host. It is the same JSON the image ships
# at /opt/jeryu/seccomp/, so the symlink syscall block this asserts is the image's own.
seccomp_profile="$root/images/agent-sandbox/seccomp/oci-docker-phase4-seccomp.json"
branch="agents/smoke/sessions/run1"

# Host-side workspace: a real git repo on the assigned branch, mounted read-write at
# /workspace exactly like a live session. The git identity + safe.directory reach git via
# env (the guard inspects argv, not env) so commit works regardless of host file ownership.
work="$(mktemp -d)"
cleanup() {
  rm -rf "$work"
  "$runtime" rmi -f "$image" >/dev/null 2>&1 || true
}
trap cleanup EXIT

git init -q "$work"
git -C "$work" -c user.email=smoke@jeryu.invalid -c user.name=smoke commit -q --allow-empty -m seed
git -C "$work" branch -q -M "$branch"

echo "agent-sandbox smoke: building $image with $runtime"
"$runtime" build -f "$root/images/agent-sandbox/Dockerfile" -t "$image" "$root"

# The EXACT hardened flags OciSpec::from_agent_job emits, plus the per-run env a live
# session injects (the pinned branch for the git guard, git identity, real git path, and
# safe.directory so the mounted repo is trusted under the non-root uid).
hard=(
  --read-only
  --tmpfs /tmp:rw,nosuid,nodev,noexec
  --cap-drop=ALL
  --security-opt no-new-privileges
  --security-opt "seccomp=$seccomp_profile"
  --user 1000:1000
  --memory 2147483648
  --pids-limit 512
  --cpu-shares 1024
  --network none
  -v "$work:/workspace:Z"
  -w /workspace
  -e "JERYU_BRANCH=$branch"
  -e JERYU_REAL_GIT=/usr/bin/git
  -e GIT_AUTHOR_NAME=smoke -e GIT_AUTHOR_EMAIL=smoke@jeryu.invalid
  -e GIT_COMMITTER_NAME=smoke -e GIT_COMMITTER_EMAIL=smoke@jeryu.invalid
  -e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=safe.directory -e GIT_CONFIG_VALUE_0='*'
)

passes=0
fails=0
OUT=""
RC=0

# Run one command inside the hardened container; capture combined output + exit code.
cell() {
  OUT="$("$runtime" run --rm "${hard[@]}" "$image" "$@" 2>&1)" && RC=0 || RC=$?
}

ok() {
  passes=$((passes + 1))
  echo "  PASS: $1"
}
bad() {
  fails=$((fails + 1))
  echo "  FAIL: $1${2:+ -- got: $2}"
}

expect_zero() {
  local desc="$1"
  shift
  cell "$@"
  if [[ "$RC" -eq 0 ]]; then ok "$desc"; else bad "$desc" "exit $RC: $OUT"; fi
}

expect_nonzero() {
  local desc="$1"
  shift
  cell "$@"
  if [[ "$RC" -ne 0 ]]; then ok "$desc"; else bad "$desc" "exit 0 (expected refusal): $OUT"; fi
}

expect_refusal() {
  local desc="$1" needle="$2"
  shift 2
  cell "$@"
  if [[ "$RC" -ne 0 && "$OUT" == *"$needle"* ]]; then
    ok "$desc"
  else
    bad "$desc" "exit $RC: $OUT"
  fi
}

echo "agent-sandbox smoke: asserting the lockdown holds"

# Read-only root: writes outside /workspace + /tmp are denied; the two writable trees work.
expect_nonzero "read-only root denies a write to /etc" sh -c 'echo x > /etc/jeryu-smoke'
expect_nonzero "read-only root denies a write to /opt" sh -c 'echo x > /opt/jeryu-smoke'
expect_zero "tmpfs /tmp is writable" sh -c 'echo x > /tmp/jeryu-smoke'
expect_zero "the mounted workspace is writable" sh -c 'echo x > /workspace/jeryu-smoke-edit'

# --network none: an outbound connection attempt cannot leave the cell.
expect_nonzero "network is denied (outbound connect fails)" \
  python3 -c 'import socket;s=socket.socket();s.settimeout(4);s.connect(("1.1.1.1",53))'

# Refusal wrappers: the disabled tools exit non-zero with the refusal message.
refusal="is disabled in the agent sandbox"
expect_refusal "ln is refused" "$refusal" ln -s /etc/passwd /workspace/lnk
expect_refusal "gh is refused" "$refusal" gh auth status
expect_refusal "curl is refused" "$refusal" curl https://example.invalid
expect_refusal "sudo is refused" "$refusal" sudo id

# The seccomp profile blocks the symlink syscall itself, not just the ln wrapper.
expect_nonzero "symlink() syscall is blocked by seccomp" \
  python3 -c 'import os;os.symlink("/etc/passwd","/workspace/seccomp-lnk")'

# The git guard (installed AS git): branch-local reads/edits pass; branch moves + push fail.
# The workspace already carries `jeryu-smoke-edit` from the writable-tree check above, so
# the guard's `add` path is exercised on that exact, named file rather than the whole tree.
expect_zero "git status is allowed" git status --short
expect_zero "git add of a named file is allowed" git add jeryu-smoke-edit
expect_zero "git commit on the assigned branch is allowed" git commit -q --allow-empty -m smoke-commit
expect_refusal "git checkout -b other is refused" "jeryu-git: refused" git checkout -b other
expect_refusal "git push is refused" "jeryu-git: refused" git push origin HEAD

# The in-image toolchain a coding agent builds this repo with is present.
expect_zero "cargo is present" cargo --version
expect_zero "node is present" node --version
expect_zero "tsc is present" tsc --version

# The pinned jankurai auditor ships in the image (the runtime is --network none, so CI
# lanes can never install it at session time). KEEP IN SYNC with ops/ci/ensure-jankurai.sh:
# the version must be the pin, the binary must resolve unshadowed at the exact path
# ops/ci/common.sh derives from CARGO_HOME, and JERYU_JANKURAI_BIN must point there so
# lanes never fall back to a bare `jankurai` PATH lookup.
expect_zero "jankurai is the pinned 1.6.10" \
  sh -c '[ "$(/opt/rust/cargo/bin/jankurai --version)" = "jankurai 1.6.10" ]'
expect_zero "jankurai resolves to the pinned path (no shadowing)" \
  sh -c '[ "$(command -v jankurai)" = "/opt/rust/cargo/bin/jankurai" ]'
expect_zero "JERYU_JANKURAI_BIN points at the pinned path" \
  sh -c '[ "$JERYU_JANKURAI_BIN" = "/opt/rust/cargo/bin/jankurai" ]'

echo "agent-sandbox smoke: $passes passed, $fails failed"
if [[ "$mode" == "full" ]]; then
  echo "agent-sandbox smoke: full mode ran the complete lockdown battery on $runtime"
fi

if [[ "$fails" -eq 0 ]]; then
  echo "agent-sandbox smoke: PASSED"
  exit 0
fi
echo "agent-sandbox smoke: FAILED"
exit 1
