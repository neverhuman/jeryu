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
# lanes can never install it at session time). KEEP IN SYNC with ops/ci/lib.sh:
# the version, digest, and installation receipt must bind the exact explicit
# path. The same governed verifier used by score/CI must pass inside the image.
expect_zero "jankurai is the pinned 1.6.11" \
  sh -c '[ "$(/opt/rust/cargo/bin/jankurai --version)" = "jankurai 1.6.11" ]'
expect_zero "jankurai is a single-link physical file" \
  sh -c '[ -f /opt/rust/cargo/bin/jankurai ] && [ ! -L /opt/rust/cargo/bin/jankurai ] && [ "$(realpath -e /opt/rust/cargo/bin/jankurai)" = /opt/rust/cargo/bin/jankurai ] && [ "$(stat -c %h /opt/rust/cargo/bin/jankurai)" = 1 ]'
expect_zero "jankurai has the governed digest" \
  sh -c '[ "$(sha256sum /opt/rust/cargo/bin/jankurai | awk '\''{print $1}'\'')" = 9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c ]'
expect_zero "jankurai resolves to the pinned path (no shadowing)" \
  sh -c '[ "$(command -v jankurai)" = "/opt/rust/cargo/bin/jankurai" ]'
expect_zero "JERYU_JANKURAI_BIN points at the pinned path" \
  sh -c '[ "$JERYU_JANKURAI_BIN" = "/opt/rust/cargo/bin/jankurai" ]'
expect_zero "JERYU_GOVERNED_JANKURAI_BIN points at the pinned path" \
  sh -c '[ "$JERYU_GOVERNED_JANKURAI_BIN" = "/opt/rust/cargo/bin/jankurai" ]'
expect_zero "JERYU_JANKURAI_RECEIPT points at the baked content-addressed receipt" \
  sh -c '[ "$JERYU_JANKURAI_RECEIPT" = "/opt/jeryu/receipts/jankurai/sha256/5e936e6c062cc6c4ef697e96b0fd1fd136ede04802667b191ac3059ac37b1d16.json" ]'
expect_zero "governed Jankurai verifier passes against the baked receipt" \
  bash /opt/jeryu/governance/ensure-jankurai.sh

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
