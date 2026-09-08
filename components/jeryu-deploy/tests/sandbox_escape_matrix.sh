#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/ops/ci/common.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

if [[ "${JERYU_SANDBOX_SKIP_STATIC:-0}" != "1" ]]; then
  cargo test -p jeryu-runner-core --jobs "${JERYU_CI_JOBS}" fscheck
  cargo test -p jeryu-runner-native --jobs "${JERYU_CI_JOBS}" guards
  cargo test -p jeryu-runner-oci --jobs "${JERYU_CI_JOBS}" oci_spec
  cargo test -p jeryu-runnerd --jobs "${JERYU_CI_JOBS}" phase4_gates
  cargo run -q -p jeryu-runnerd --jobs "${JERYU_CI_JOBS}" -- explain examples/jobs/denied-native-hot-fork.job >/tmp/jeryu-denied.json || status=$?
  status="${status:-0}"
  if [[ "$status" != "3" ]]; then
    echo "expected denied-native-hot-fork to exit 3, got $status" >&2
    exit 1
  fi
  echo "runner-sandbox static guard matrix: PASS"
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required for the live namespace/seccomp/cgroup escape matrix" >&2
  exit 1
fi

IMAGE="${JERYU_SANDBOX_IMAGE:-alpine:3.20}"
if ! docker image inspect "${IMAGE}" >/dev/null 2>&1; then
  docker pull "${IMAGE}"
fi

ARTIFACT_DIR="${ROOT}/target/jankurai/runner-sandbox"
mkdir -p "${ARTIFACT_DIR}"
WORKSPACE="$(mktemp -d)"
trap 'rm -rf "${WORKSPACE}"' EXIT

RESULTS=()
FAILURES=0

record() {
  local name="$1"
  local status="$2"
  RESULTS+=("{\"name\":\"${name}\",\"status\":\"${status}\"}")
  if [[ "${status}" != "pass" ]]; then
    FAILURES=$((FAILURES + 1))
  fi
}

run_pass() {
  local name="$1"
  shift
  if "$@"; then
    record "${name}" "pass"
  else
    record "${name}" "fail"
  fi
}

run_fail() {
  local name="$1"
  shift
  if "$@"; then
    record "${name}" "fail"
  else
    record "${name}" "pass"
  fi
}

docker_sandbox() {
  docker run --rm \
    --network none \
    --pids-limit 64 \
    --memory 64m \
    --cpus 1 \
    --security-opt no-new-privileges \
    --read-only \
    --tmpfs /tmp:rw,noexec,nosuid,size=16m \
    -v "${WORKSPACE}:/workspace:rw" \
    -w /workspace \
    "${IMAGE}" "$@"
}

run_pass workspace_write_allowed_root_readonly \
  docker_sandbox sh -c 'touch inside-ok && ! touch /outside-denied && test -f inside-ok'

run_pass host_socket_absent \
  docker_sandbox sh -c 'test ! -S /var/run/docker.sock && test ! -S /run/docker.sock'

run_pass credential_env_cleared \
  docker_sandbox sh -c 'test -z "${SSH_AUTH_SOCK:-}" && test -z "${AWS_ACCESS_KEY_ID:-}" && test -z "${GITHUB_TOKEN:-}"'

run_pass network_denied_egress \
  docker_sandbox sh -c '! wget -q -T 1 -O /tmp/net.out http://1.1.1.1/'

run_fail pids_limit_enforced \
  docker run --rm --network none --pids-limit 32 --memory 64m --security-opt no-new-privileges --read-only --tmpfs /tmp:rw,noexec,nosuid,size=16m "${IMAGE}" \
    sh -c 'i=0; while [ $i -lt 100 ]; do sleep 20 & i=$((i+1)); done; wait'

run_fail memory_limit_enforced \
  docker run --rm --network none --pids-limit 64 --memory 32m --security-opt no-new-privileges --read-only --tmpfs /tmp:rw,noexec,nosuid,size=16m "${IMAGE}" \
    sh -c 'dd if=/dev/zero bs=1M count=96 2>/dev/null | tr "\0" x | awk "{a=a \$0} END{print length(a)}" >/tmp/mem.out'

run_fail denied_syscall_unshare \
  docker_sandbox sh -c 'unshare -U true'

run_pass no_new_privs_enforced \
  docker_sandbox sh -c "grep -Eq '^NoNewPrivs:[[:space:]]+1$' /proc/self/status"

run_pass cgroup_limits_visible \
  docker_sandbox sh -c 'test -f /sys/fs/cgroup/memory.max || test -f /sys/fs/cgroup/memory/memory.limit_in_bytes'

joined=""
for result in "${RESULTS[@]}"; do
  if [[ -n "${joined}" ]]; then
    joined+=","
  fi
  joined+="${result}"
done

cat >"${ARTIFACT_DIR}/live-matrix.json" <<JSON
{
  "schema": "jeryu.runner-sandbox.live-matrix.v1",
  "runtime": "docker",
  "image": "${IMAGE}",
  "isolation": ["network-none", "pids-limit", "memory-limit", "no-new-privileges", "default-seccomp", "read-only-root", "workspace-only-write-bind"],
  "results": [${joined}],
  "failures": ${FAILURES}
}
JSON

if [[ "${FAILURES}" != "0" ]]; then
  echo "runner-sandbox live escape matrix: FAIL (${FAILURES} failing checks)"
  exit 1
fi

echo "runner-sandbox live escape matrix: PASS"
