#!/usr/bin/env bash
# Verify CI is using this repo's Jeryu surfaces, not legacy local state.
set -euo pipefail

build_local=0
legacy_guard=0
for arg in "$@"; do
  case "${arg}" in
    --build-local) build_local=1 ;;
    --legacy-guard) legacy_guard=1 ;;
    *) echo "unknown argument: ${arg}" >&2; exit 2 ;;
  esac
done

ROOT="$(git rev-parse --show-toplevel)"
cd "${ROOT}"

if [ "${GITHUB_ACTIONS:-}" != "true" ]; then
  expected="${JERYU_CANONICAL_ROOT:-/home/ubuntu/jeryuRUST}"
  if [ -d "${expected}" ]; then
    actual_real="$(realpath "${ROOT}")"
    expected_real="$(realpath "${expected}")"
    if [ "${actual_real}" != "${expected_real}" ]; then
      echo "wrong Jeryu root: got ${actual_real}, want ${expected_real}" >&2
      exit 1
    fi
  fi
  case "$(realpath "${ROOT}")" in
    */jeryu_rust) echo "wrong Jeryu root: legacy /home/ubuntu/jeryu_rust is not canonical" >&2; exit 1 ;;
  esac
fi

remote="$(git remote get-url origin 2>/dev/null || true)"
case "${remote}" in
  ""|git@github.com:neverhuman/jeryu.git|https://github.com/neverhuman/jeryu|https://github.com/neverhuman/jeryu.git)
    ;;
  *)
    echo "noncanonical origin remote: ${remote}" >&2
    exit 1
    ;;
esac

check_legacy_processes() {
  [ "${GITHUB_ACTIONS:-}" = "true" ] && return 0
  [ "${JERYU_CI_ALLOW_LEGACY_PROCESSES:-0}" = "1" ] && return 0

  local hits
  hits="$(
    ps -eo pid=,comm=,args= |
      grep -E 'gitlab-runner|/opt/gitlab/|/home/ubuntu/\.jeryu/bin/|/home/ubuntu/jeryu/target/|/home/ubuntu/jeryu_rust/' |
      grep -v 'grep -E' || true
  )"
  if [ -n "${hits}" ]; then
    echo "legacy Jeryu/GitLab processes are active during release validation:" >&2
    printf '%s\n' "${hits}" | sed 's/^/  /' >&2
    echo "stop or quarantine the legacy services before running the full release gate" >&2
    return 1
  fi
}

is_repo_jeryu_pid() {
  local pid="$1"
  local exe
  exe="$(readlink -f "/proc/${pid}/exe" 2>/dev/null || true)"
  case "${exe}" in
    "${ROOT}/target/debug/jeryu"|\
    "${ROOT}/target/release/jeryu")
      return 0
      ;;
  esac
  return 1
}

check_legacy_listeners() {
  [ "${GITHUB_ACTIONS:-}" = "true" ] && return 0
  [ "${JERYU_CI_ALLOW_LEGACY_LISTENERS:-0}" = "1" ] && return 0
  command -v ss >/dev/null 2>&1 || return 0

  local ports=(2224 8787 18787 18788 19800)
  local failed=0
  local line state recv send local_addr peer process port pid
  while IFS= read -r line; do
    read -r state recv send local_addr peer process <<<"${line}"
    for port in "${ports[@]}"; do
      case "${local_addr}" in
        *":${port}")
          if [[ "${line}" =~ pid=([0-9]+) ]]; then
            pid="${BASH_REMATCH[1]}"
            if is_repo_jeryu_pid "${pid}"; then
              continue
            fi
            echo "legacy or noncanonical listener on ${local_addr}: ${line}" >&2
            echo "  pid ${pid}: $(ps -p "${pid}" -o args= 2>/dev/null || true)" >&2
          else
            echo "legacy or unowned listener on ${local_addr}: ${line}" >&2
          fi
          failed=1
          ;;
      esac
    done
  done < <(ss -H -ltnp 2>/dev/null || true)

  if [ "${failed}" -ne 0 ]; then
    echo "stop or reassign legacy listeners on ports: ${ports[*]}" >&2
    return 1
  fi
}

check_legacy_remotes() {
  [ "${JERYU_CI_ALLOW_LEGACY_REMOTES:-0}" = "1" ] && return 0
  local hits
  hits="$(
    git remote -v |
      grep -E '127\.0\.0\.1:2224|localhost:2224|/home/ubuntu/\.jeryu|/home/ubuntu/jeryu/|gitlab' || true
  )"
  if [ -n "${hits}" ]; then
    echo "legacy remotes are configured during release validation:" >&2
    printf '%s\n' "${hits}" | sed 's/^/  /' >&2
    return 1
  fi
}

legacy_path=""
if path_jeryu="$(command -v jeryu 2>/dev/null)"; then
  case "${path_jeryu}" in
    "${HOME}/.jeryu/"*) legacy_path="${path_jeryu}" ;;
  esac
fi

if [ "${build_local}" = "1" ]; then
  cargo build -q -p jeryu-cli --bin jeryu --jobs "${JERYU_CI_JOBS:-40}"
fi

repo_bin=""
for candidate in "${ROOT}/target/debug/jeryu" "${ROOT}/target/release/jeryu"; do
  if [ -x "${candidate}" ]; then
    repo_bin="${candidate}"
    break
  fi
done

if [ -z "${repo_bin}" ]; then
  echo "repo-built jeryu binary not found; run cargo build -p jeryu-cli --bin jeryu" >&2
  exit 1
fi

version="$("${repo_bin}" --version)"
case "${version}" in
  jeryu\ *) ;;
  *) echo "unexpected repo jeryu version output: ${version}" >&2; exit 1 ;;
esac

echo "jeryu repo binary ok: ${repo_bin} (${version})"
if [ -n "${legacy_path}" ]; then
  echo "legacy PATH jeryu ignored: ${legacy_path}"
fi
if [ -n "${remote}" ]; then
  echo "origin remote ok: ${remote}"
fi

if [ "${legacy_guard}" = "1" ]; then
  legacy_fail=0
  check_legacy_remotes || legacy_fail=1
  check_legacy_processes || legacy_fail=1
  check_legacy_listeners || legacy_fail=1
  if [ "${legacy_fail}" -ne 0 ]; then
    exit 1
  fi
  echo "legacy process/listener guard ok"
fi
