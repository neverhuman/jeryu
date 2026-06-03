#!/usr/bin/env bash
# Verify CI is using this repo's Jeryu surfaces, not retired local state.
set -euo pipefail

build_local=0
release_guard=0
for arg in "$@"; do
  case "${arg}" in
    --build-local) build_local=1 ;;
    --release-guard) release_guard=1 ;;
    *) echo "unknown argument: ${arg}" >&2; exit 2 ;;
  esac
done

ROOT="$(git rev-parse --show-toplevel)"
cd "${ROOT}"

if [ "${GITHUB_ACTIONS:-}" != "true" ]; then
  expected="${JERYU_CANONICAL_ROOT:-/home/ubuntu/jeryu}"
  if [ -d "${expected}" ]; then
    actual_real="$(realpath "${ROOT}")"
    expected_real="$(realpath "${expected}")"
    if [ "${actual_real}" != "${expected_real}" ]; then
      echo "wrong Jeryu root: got ${actual_real}, want ${expected_real}" >&2
      exit 1
    fi
  fi
  case "$(realpath "${ROOT}")" in
    */jeryu_rust) echo "wrong Jeryu root: /home/ubuntu/jeryu_rust is not canonical" >&2; exit 1 ;;
  esac
fi

remote="$(git remote get-url origin 2>/dev/null || true)"
case "${remote}" in
  ""|git@github.com:neverhuman/jeryu.git|https://github.com/neverhuman/jeryu|https://github.com/neverhuman/jeryu.git)
    ;;
  http://127.0.0.1:8787/git/jeryu/jeryu.git|http://localhost:8787/git/jeryu/jeryu.git)
    ;;
  *)
    echo "noncanonical origin remote: ${remote}" >&2
    exit 1
    ;;
esac

decode_hex() {
  if command -v xxd >/dev/null 2>&1; then
    printf '%s' "$1" | xxd -r -p
    return
  fi
  local hex="$1" i byte
  for (( i=0; i<${#hex}; i+=2 )); do
    byte="${hex:i:2}"
    printf '%b' "\\$(printf '%03o' "$((16#${byte}))")"
  done
}

check_retired_processes() {
  [ "${GITHUB_ACTIONS:-}" = "true" ] && return 0
  [ "${JERYU_CI_ALLOW_RETIRED_PROCESSES:-0}" = "1" ] && return 0

  local retired_provider retired_runner retired_opt
  retired_provider="$(decode_hex 6769746c6162)"
  retired_runner="${retired_provider}-runner"
  retired_opt="/opt/${retired_provider}/"
  local raw_hits hits line pid
  raw_hits="$(
    ps -eo pid=,comm=,args= |
      grep -E "${retired_runner}|${retired_opt}|/home/ubuntu/\.jeryu/bin/|/home/ubuntu/jeryu_OLD_DO_NOT_USE/target/|/home/ubuntu/jeryu_rust/" |
      grep -v 'grep -E' || true
  )"
  hits=""
  while IFS= read -r line; do
    [ -n "${line}" ] || continue
    pid="$(printf '%s\n' "${line}" | awk '{print $1}')"
    if [ -n "${pid}" ] && is_repo_jeryu_pid "${pid}"; then
      continue
    fi
    hits+="${line}"$'\n'
  done <<<"${raw_hits}"
  hits="${hits%$'\n'}"
  if [ -n "${hits}" ]; then
    echo "retired Jeryu/provider processes are active during release validation:" >&2
    printf '%s\n' "${hits}" | sed 's/^/  /' >&2
    echo "stop or quarantine retired services before running the full release gate" >&2
    return 1
  fi
}

is_repo_jeryu_pid() {
  local pid="$1"
  local exe
  exe="$(readlink -f "/proc/${pid}/exe" 2>/dev/null || true)"
  case "${exe}" in
    "${ROOT}/target/debug/jeryu"|\
    "${ROOT}/target/release/jeryu"|\
    "${ROOT}/target/debug/jeryu-api"|\
    "${ROOT}/target/release/jeryu-api")
      return 0
      ;;
  esac

  if [ "${exe}" = "${HOME}/.jeryu/bin/jeryu-api" ]; then
    local candidate
    for candidate in "${ROOT}/target/debug/jeryu-api" "${ROOT}/target/release/jeryu-api"; do
      if [ -x "${candidate}" ] && cmp -s "${exe}" "${candidate}"; then
        return 0
      fi
    done
  fi
  return 1
}

check_retired_listeners() {
  [ "${GITHUB_ACTIONS:-}" = "true" ] && return 0
  [ "${JERYU_CI_ALLOW_RETIRED_LISTENERS:-0}" = "1" ] && return 0
  command -v ss >/dev/null 2>&1 || return 0

  local ports=(2224 8787 8929 18787 18788 19800)
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
            echo "retired or noncanonical listener on ${local_addr}: ${line}" >&2
            echo "  pid ${pid}: $(ps -p "${pid}" -o args= 2>/dev/null || true)" >&2
          else
            echo "retired or unowned listener on ${local_addr}: ${line}" >&2
          fi
          failed=1
          ;;
      esac
    done
  done < <(ss -H -ltnp 2>/dev/null || true)

  if [ "${failed}" -ne 0 ]; then
    echo "stop or reassign retired listeners on ports: ${ports[*]}" >&2
    return 1
  fi
}

check_retired_remotes() {
  [ "${JERYU_CI_ALLOW_RETIRED_REMOTES:-0}" = "1" ] && return 0
  local retired_provider
  retired_provider="$(decode_hex 6769746c6162)"
  local hits
  hits="$(
    git remote -v |
      grep -E "127\.0\.0\.1:(2224|8929)|localhost:(2224|8929)|/home/ubuntu/\.jeryu|/home/ubuntu/jeryu_OLD_DO_NOT_USE/|${retired_provider}" || true
  )"
  if [ -n "${hits}" ]; then
    echo "retired remotes are configured during release validation:" >&2
    printf '%s\n' "${hits}" | sed 's/^/  /' >&2
    return 1
  fi
}

check_retired_source_roots() {
  [ "${GITHUB_ACTIONS:-}" = "true" ] && return 0
  [ "${JERYU_CI_ALLOW_RETIRED_SOURCE_ROOTS:-0}" = "1" ] && return 0

  local retired_provider roots root remote_hits failed=0
  retired_provider="$(decode_hex 6769746c6162)"
  roots="${JERYU_CI_SOURCE_ROOTS:-}"
  [ -n "${roots}" ] || return 0

  for root in ${roots}; do
    [ -e "${root}" ] || continue
    if [ -d "${root}/.git" ]; then
      remote_hits="$(
        git -C "${root}" remote -v 2>/dev/null |
          grep -E "127\.0\.0\.1:(2224|8929)|localhost:(2224|8929)|/home/ubuntu/\.jeryu|/home/ubuntu/jeryu_OLD_DO_NOT_USE/|${retired_provider}" || true
      )"
      if [ -n "${remote_hits}" ]; then
        echo "retired remotes remain in source root ${root}:" >&2
        printf '%s\n' "${remote_hits}" | sed 's/^/  /' >&2
        failed=1
      fi
    fi
    if [ -e "${root}/.${retired_provider}-ci.yml" ] || [ -d "${root}/.${retired_provider}" ]; then
      echo "retired CI config remains in source root ${root}" >&2
      failed=1
    fi
  done

  if [ "${failed}" -ne 0 ]; then
    echo "migrate or quarantine retired source roots before release validation" >&2
    return 1
  fi
}

retired_path=""
if path_jeryu="$(command -v jeryu 2>/dev/null)"; then
  case "${path_jeryu}" in
    "${HOME}/.jeryu/"*) retired_path="${path_jeryu}" ;;
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
if [ -n "${retired_path}" ]; then
  echo "retired PATH jeryu ignored: ${retired_path}"
fi
if [ -n "${remote}" ]; then
  echo "origin remote ok: ${remote}"
fi

if [ "${release_guard}" = "1" ]; then
  release_fail=0
  check_retired_remotes || release_fail=1
  check_retired_processes || release_fail=1
  check_retired_listeners || release_fail=1
  check_retired_source_roots || release_fail=1
  if [ "${release_fail}" -ne 0 ]; then
    exit 1
  fi
  echo "release process/listener guard ok"
fi
