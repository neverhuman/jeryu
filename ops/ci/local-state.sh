#!/usr/bin/env bash
# Classify and repair local release-validation state.
set -euo pipefail

MODE="verify"
SUMMARY_PATH=""

usage() {
  cat <<'USAGE'
usage: bash ops/ci/local-state.sh [--scan|--verify|--repair] [--summary PATH]

Classifies local release-validation state. --repair only stops allowlisted,
repo-owned Jeryu API dev/test processes; unknown listeners and retired state
remain hard blockers.
USAGE
}

while [ "$#" -gt 0 ]; do
  arg="$1"
  case "${arg}" in
    --scan) MODE="scan" ;;
    --verify) MODE="verify" ;;
    --repair) MODE="repair" ;;
    --summary=*) SUMMARY_PATH="${arg#--summary=}" ;;
    --summary)
      shift
      [ "$#" -gt 0 ] || { echo "--summary requires a path" >&2; exit 2; }
      SUMMARY_PATH="$1"
      ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: ${arg}" >&2; exit 2 ;;
  esac
  shift
done

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
if [ -z "${ROOT}" ]; then
  ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fi
cd "${ROOT}"

if [ -z "${SUMMARY_PATH}" ]; then
  SUMMARY_PATH="target/ci-fast/local-state-${MODE}.json"
fi

mkdir -p "$(dirname "${SUMMARY_PATH}")"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

AUTO_STOPPED="${TMP_DIR}/auto-stopped.jsonl"
REPAIRABLE="${TMP_DIR}/repairable.jsonl"
BLOCKERS="${TMP_DIR}/blockers.jsonl"
OBSERVATIONS="${TMP_DIR}/observations.jsonl"
SEEN_SAFE_PIDS="${TMP_DIR}/seen-safe-pids"
RUNNER_STATE="${TMP_DIR}/runner-state"
API_LISTENER_STATE="${TMP_DIR}/api-listener-state"

: > "${AUTO_STOPPED}"
: > "${REPAIRABLE}"
: > "${BLOCKERS}"
: > "${OBSERVATIONS}"
: > "${SEEN_SAFE_PIDS}"
printf 'absent\n' > "${RUNNER_STATE}"
printf 'no_api_listener\n' > "${API_LISTENER_STATE}"

GUARDED_PORTS=(${JERYU_LOCAL_STATE_GUARDED_PORTS:-2224 8787 8929 18787 18788 19800})
REPAIR_COMMAND="bash ops/ci/local-state.sh --repair --summary target/ci-fast/local-state-repair.json"
RERUN_COMMAND="just closeout"
RUNNER_PROOF_COMMAND="cargo test -p jeryu-runnerd workcell --jobs 40"

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

json_string() {
  jq -Rn --arg v "$1" '$v'
}

sha256_text() {
  printf '%s' "$1" | sha256sum | awk '{print "sha256:" $1}'
}

append_observation() {
  local kind="$1" detail="$2"
  jq -cn \
    --arg kind "${kind}" \
    --arg detail "${detail}" \
    '{kind: $kind, detail: $detail}' >> "${OBSERVATIONS}"
}

append_blocker() {
  local kind="$1" reason="$2" bind_address="$3" pid="$4" executable="$5" repair_command="$6"
  if [ -n "${pid}" ]; then
    jq -cn \
      --arg kind "${kind}" \
      --arg reason "${reason}" \
      --arg bind_address "${bind_address}" \
      --arg pid "${pid}" \
      --arg executable_path "${executable}" \
      --arg repair_command "${repair_command}" \
      --arg rerun_command "${RERUN_COMMAND}" \
      '{
        kind: $kind,
        reason: $reason,
        bind_address: (if $bind_address == "" then null else $bind_address end),
        pid: ($pid | tonumber),
        executable_path: (if $executable_path == "" then null else $executable_path end),
        repair_command: (if $repair_command == "" then null else $repair_command end),
        rerun_command: $rerun_command
      }' >> "${BLOCKERS}"
  else
    jq -cn \
      --arg kind "${kind}" \
      --arg reason "${reason}" \
      --arg bind_address "${bind_address}" \
      --arg repair_command "${repair_command}" \
      --arg rerun_command "${RERUN_COMMAND}" \
      '{
        kind: $kind,
        reason: $reason,
        bind_address: (if $bind_address == "" then null else $bind_address end),
        pid: null,
        executable_path: null,
        repair_command: (if $repair_command == "" then null else $repair_command end),
        rerun_command: $rerun_command
      }' >> "${BLOCKERS}"
  fi
  printf 'blocked\n' > "${API_LISTENER_STATE}"
}

append_repairable() {
  local pid="$1" executable="$2" bind_address="$3" reason="$4" args="$5"
  jq -cn \
    --arg pid "${pid}" \
    --arg executable_path "${executable}" \
    --arg bind_address "${bind_address}" \
    --arg reason "${reason}" \
    --arg command_line_digest "$(sha256_text "${args}")" \
    --arg repair_command "${REPAIR_COMMAND}" \
    '{
      pid: ($pid | tonumber),
      executable_path: $executable_path,
      bind_address: $bind_address,
      reason: $reason,
      command_line_digest: $command_line_digest,
      repair_command: $repair_command
    }' >> "${REPAIRABLE}"
}

append_auto_stopped() {
  local pid="$1" executable="$2" bind_address="$3" reason="$4" args="$5"
  jq -cn \
    --arg pid "${pid}" \
    --arg executable_path "${executable}" \
    --arg bind_address "${bind_address}" \
    --arg reason "${reason}" \
    --arg command_line_digest "$(sha256_text "${args}")" \
    '{
      pid: ($pid | tonumber),
      executable_path: $executable_path,
      bind_address: $bind_address,
      reason: $reason,
      command_line_digest: $command_line_digest
    }' >> "${AUTO_STOPPED}"
  printf 'release_validation_idle\n' > "${API_LISTENER_STATE}"
}

ps_all() {
  if [ -n "${JERYU_LOCAL_STATE_PS_FIXTURE:-}" ]; then
    sed '/^[[:space:]]*$/d' "${JERYU_LOCAL_STATE_PS_FIXTURE}"
    return
  fi
  ps -eo pid=,comm=,args=
}

ss_all() {
  if [ -n "${JERYU_LOCAL_STATE_SS_FIXTURE:-}" ]; then
    sed '/^[[:space:]]*$/d' "${JERYU_LOCAL_STATE_SS_FIXTURE}"
    return
  fi
  command -v ss >/dev/null 2>&1 || return 0
  ss -H -ltnp 2>/dev/null || true
}

ps_line_for_pid() {
  local pid="$1"
  ps_all | awk -v want="${pid}" '$1 == want { print; exit }'
}

process_pid() {
  awk '{print $1}' <<< "$1"
}

process_comm() {
  awk '{print $2}' <<< "$1"
}

process_args() {
  awk '{$1=""; $2=""; sub(/^[[:space:]]+/, ""); print}' <<< "$1"
}

command_for_pid() {
  local pid="$1" line
  if [ -n "${JERYU_LOCAL_STATE_PS_FIXTURE:-}" ]; then
    line="$(ps_line_for_pid "${pid}")"
    [ -n "${line}" ] || return 0
    process_args "${line}"
    return
  fi
  ps -p "${pid}" -o args= 2>/dev/null || true
}

exe_for_pid() {
  local pid="$1" args="$2" first
  if [ -z "${JERYU_LOCAL_STATE_PS_FIXTURE:-}" ]; then
    readlink -f "/proc/${pid}/exe" 2>/dev/null && return
  fi
  first="${args%% *}"
  case "${first}" in
    /*) printf '%s\n' "${first}" ;;
    *) printf '%s\n' "${first}" ;;
  esac
}

is_repo_executable() {
  local exe="$1"
  case "${exe}" in
    "${ROOT}/target/debug/jeryu"|\
    "${ROOT}/target/release/jeryu"|\
    "${ROOT}/target/debug/jeryu-api"|\
    "${ROOT}/target/release/jeryu-api")
      return 0
      ;;
  esac
  return 1
}

is_api_serve_args() {
  local args=" $1 "
  case "${args}" in
    *" web serve "*) return 0 ;;
  esac
  return 1
}

is_repo_api_process() {
  local exe="$1" args="$2"
  is_repo_executable "${exe}" && is_api_serve_args "${args}"
}

uses_retired_data_dir() {
  local args="$1"
  case "${args}" in
    *"--data-dir ${HOME}/.jeryu"*|\
    *"--data-dir=${HOME}/.jeryu"*|\
    *"--data-dir ~/.jeryu"*|\
    *"--data-dir=~/.jeryu"*)
      return 0
      ;;
  esac
  return 1
}

is_runnerd_process() {
  local comm="$1" exe="$2" args="$3"
  case "${comm}" in
    jeryu-runnerd) return 0 ;;
  esac
  case "${exe##*/}" in
    jeryu-runnerd) return 0 ;;
  esac
  case " ${args} " in
    *" jeryu-runnerd "*) return 0 ;;
  esac
  return 1
}

listener_pid() {
  local line="$1"
  if [[ "${line}" =~ pid=([0-9]+) ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
  fi
}

listener_port() {
  local addr="$1"
  if [[ "${addr}" =~ :([0-9]+)$ ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
  fi
}

is_guarded_port() {
  local port="$1" guarded
  for guarded in "${GUARDED_PORTS[@]}"; do
    [ "${port}" = "${guarded}" ] && return 0
  done
  return 1
}

stop_repo_process() {
  local pid="$1"
  if [ -n "${JERYU_LOCAL_STATE_PS_FIXTURE:-}" ]; then
    return 0
  fi
  kill "${pid}" 2>/dev/null || return 0
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    if ! kill -0 "${pid}" 2>/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  return 1
}

handle_safe_api() {
  local pid="$1" executable="$2" bind_address="$3" reason="$4" args="$5"
  if grep -qx "${pid}" "${SEEN_SAFE_PIDS}"; then
    return 0
  fi
  printf '%s\n' "${pid}" >> "${SEEN_SAFE_PIDS}"
  case "${MODE}" in
    repair)
      if stop_repo_process "${pid}"; then
        append_auto_stopped "${pid}" "${executable}" "${bind_address}" "${reason}" "${args}"
      else
        append_blocker "auto_stop_failed" "repo-owned API process did not stop after TERM" "${bind_address}" "${pid}" "${executable}" "${REPAIR_COMMAND}"
      fi
      ;;
    scan)
      append_repairable "${pid}" "${executable}" "${bind_address}" "${reason}" "${args}"
      printf 'repairable_local_api_active\n' > "${API_LISTENER_STATE}"
      ;;
    verify)
      append_repairable "${pid}" "${executable}" "${bind_address}" "${reason}" "${args}"
      printf 'repair_required\n' > "${API_LISTENER_STATE}"
      ;;
  esac
}

retired_url_or_path() {
  local value="$1" retired_provider
  retired_provider="$(decode_hex 6769746c6162)"
  case "${value}" in
    *"127.0.0.1:2224"*|\
    *"127.0.0.1:8929"*|\
    *"localhost:2224"*|\
    *"localhost:8929"*|\
    *"${HOME}/.jeryu"*|\
    *"/home/ubuntu/jeryu_OLD_DO_NOT_USE/"*|\
    *"${retired_provider}"*)
      return 0
      ;;
  esac
  return 1
}

classify_remotes() {
  [ "${JERYU_CI_ALLOW_RETIRED_REMOTES:-0}" = "1" ] && return 0
  local seen="${TMP_DIR}/seen-remotes" name url kind key
  : > "${seen}"
  while read -r name url kind; do
    [ -n "${name}" ] || continue
    retired_url_or_path "${url}" || continue
    key="${name}:${url}"
    grep -qxF "${key}" "${seen}" && continue
    printf '%s\n' "${key}" >> "${seen}"
    append_blocker "retired_remote" "retired remote endpoint configured" "" "" "" "git remote set-url ${name} git@github.com:neverhuman/jeryu.git"
  done < <(git remote -v 2>/dev/null || true)
}

classify_source_roots() {
  [ "${GITHUB_ACTIONS:-}" = "true" ] && return 0
  [ "${JERYU_CI_ALLOW_RETIRED_SOURCE_ROOTS:-0}" = "1" ] && return 0

  local retired_provider roots root remote_hit
  retired_provider="$(decode_hex 6769746c6162)"
  roots="${JERYU_CI_SOURCE_ROOTS:-/home/ubuntu/redlineDB /home/ubuntu/redline-testing /home/ubuntu/openQG /home/ubuntu/jekko}"

  for root in ${roots}; do
    [ -e "${root}" ] || continue
    if [ -d "${root}/.git" ]; then
      remote_hit="$(
        git -C "${root}" remote -v 2>/dev/null |
          while read -r _ url _; do
            if retired_url_or_path "${url}"; then
              printf 'yes\n'
              break
            fi
          done
      )"
      if [ -n "${remote_hit}" ]; then
        append_blocker "retired_source_root_remote" "retired remote remains in monitored source root ${root}" "" "" "" ""
      fi
    fi
    if [ -e "${root}/.${retired_provider}-ci.yml" ] || [ -d "${root}/.${retired_provider}" ]; then
      append_blocker "retired_source_root_config" "retired CI config remains in monitored source root ${root}" "" "" "" ""
    fi
  done
}

classify_processes() {
  [ "${GITHUB_ACTIONS:-}" = "true" ] && return 0
  [ "${JERYU_CI_ALLOW_RETIRED_PROCESSES:-0}" = "1" ] && return 0

  local retired_provider retired_runner retired_opt line pid comm args exe
  retired_provider="$(decode_hex 6769746c6162)"
  retired_runner="${retired_provider}-runner"
  retired_opt="/opt/${retired_provider}/"

  while IFS= read -r line; do
    [ -n "${line}" ] || continue
    read -r pid comm args <<< "${line}"
    args="${args:-}"
    exe=""

    if is_runnerd_process "${comm}" "${exe}" "${args}"; then
      printf 'present\n' > "${RUNNER_STATE}"
    fi

    if is_api_serve_args "${args}" && uses_retired_data_dir "${args}"; then
      exe="$(exe_for_pid "${pid}" "${args}")"
      if is_repo_api_process "${exe}" "${args}"; then
        handle_safe_api "${pid}" "${exe}" "not_listening" "repo_owned_api_retired_data_dir" "${args}"
      else
        append_blocker "retired_data_dir_nonrepo_api" "non-repo API process uses retired data dir" "" "${pid}" "${exe}" ""
      fi
      continue
    fi

    case "${args}" in
      *"${retired_runner}"*|\
      *"${retired_opt}"*|\
      *"${HOME}/.jeryu/bin/"*|\
      *"/home/ubuntu/jeryu_OLD_DO_NOT_USE/target/"*|\
      *"/home/ubuntu/jeryu_rust/"*)
        exe="$(exe_for_pid "${pid}" "${args}")"
        append_blocker "retired_process" "retired process is active during release validation" "" "${pid}" "${exe}" ""
        ;;
    esac
  done < <(ps_all)
}

classify_listeners() {
  [ "${GITHUB_ACTIONS:-}" = "true" ] && return 0
  [ "${JERYU_CI_ALLOW_RETIRED_LISTENERS:-0}" = "1" ] && return 0

  local line state recv send local_addr peer rest port pid args exe
  while IFS= read -r line; do
    [ -n "${line}" ] || continue
    read -r state recv send local_addr peer rest <<< "${line}"
    port="$(listener_port "${local_addr}")"
    [ -n "${port}" ] || continue
    is_guarded_port "${port}" || continue

    pid="$(listener_pid "${line}")"
    if [ -z "${pid}" ]; then
      append_blocker "unknown_listener" "guarded port listener has no owning pid" "${local_addr}" "" "" ""
      continue
    fi

    args="$(command_for_pid "${pid}")"
    exe="$(exe_for_pid "${pid}" "${args}")"
    if is_repo_api_process "${exe}" "${args}"; then
      if uses_retired_data_dir "${args}"; then
        handle_safe_api "${pid}" "${exe}" "${local_addr}" "repo_owned_api_retired_data_dir" "${args}"
      else
        handle_safe_api "${pid}" "${exe}" "${local_addr}" "repo_owned_api_listener" "${args}"
      fi
      continue
    fi

    append_blocker "unknown_listener" "guarded port listener is not an allowlisted repo-owned API process" "${local_addr}" "${pid}" "${exe}" ""
  done < <(ss_all)
}

classify_runtime_observations() {
  if [ "$(cat "${RUNNER_STATE}")" = "absent" ]; then
    append_observation "runnerd_absent_non_blocking" "live jeryu-runnerd is not required for local closeout"
  else
    append_observation "runnerd_present_non_blocking" "live jeryu-runnerd is observed but release proof remains test-backed"
  fi
  append_observation "zero_workcells_non_blocking" "zero live workcells is not a closeout blocker"
}

json_array_from_file() {
  local file="$1"
  if [ -s "${file}" ]; then
    jq -s . "${file}"
  else
    printf '[]'
  fi
}

write_summary() {
  local auto_count repairable_count blocker_count ok release_posture api_state runner_state
  auto_count="$(jq -s 'length' "${AUTO_STOPPED}")"
  repairable_count="$(jq -s 'length' "${REPAIRABLE}")"
  blocker_count="$(jq -s 'length' "${BLOCKERS}")"
  api_state="$(cat "${API_LISTENER_STATE}")"
  runner_state="$(cat "${RUNNER_STATE}")"

  release_posture="release_validation_idle"
  if [ "${blocker_count}" -ne 0 ]; then
    release_posture="blocked"
  elif [ "${MODE}" = "verify" ] && [ "${repairable_count}" -ne 0 ]; then
    release_posture="blocked"
  elif [ "${api_state}" = "repairable_local_api_active" ]; then
    release_posture="repairable_local_api_active"
  fi

  ok="true"
  if [ "${blocker_count}" -ne 0 ]; then
    ok="false"
  elif [ "${MODE}" = "verify" ] && [ "${repairable_count}" -ne 0 ]; then
    ok="false"
  fi

  jq -n \
    --arg schema "jeryu.local-state.v1" \
    --arg mode "${MODE}" \
    --arg root "${ROOT}" \
    --arg release_posture "${release_posture}" \
    --arg api_listener_state "${api_state}" \
    --arg repair_command "${REPAIR_COMMAND}" \
    --arg rerun_command "${RERUN_COMMAND}" \
    --arg runner_state "${runner_state}" \
    --arg runner_proof_command "${RUNNER_PROOF_COMMAND}" \
    --argjson ok "${ok}" \
    --argjson guarded_ports "$(printf '%s\n' "${GUARDED_PORTS[@]}" | jq -R 'tonumber' | jq -s .)" \
    --argjson auto_stopped "$(json_array_from_file "${AUTO_STOPPED}")" \
    --argjson repairable "$(json_array_from_file "${REPAIRABLE}")" \
    --argjson blockers "$(json_array_from_file "${BLOCKERS}")" \
    --argjson observations "$(json_array_from_file "${OBSERVATIONS}")" \
    '{
      schema: $schema,
      mode: $mode,
      root: $root,
      ok: $ok,
      guarded_ports: $guarded_ports,
      release_posture: $release_posture,
      api_listener_state: $api_listener_state,
      repair_command: $repair_command,
      rerun_command: $rerun_command,
      auto_stopped: $auto_stopped,
      repairable: $repairable,
      blockers: $blockers,
      runtime_observations: $observations,
      runner_posture: {
        live_runner_required: false,
        live_runner_state: $runner_state,
        workcells_required: false,
        zero_workcells_block_closeout: false,
        proof: {
          command: $runner_proof_command,
          deterministic_slots: 40,
          fleet_shape: "xbabe0..xbabe3 x 10 slots"
        }
      }
    }' > "${SUMMARY_PATH}"
}

print_text_summary() {
  if jq -e '.blockers | length > 0' "${SUMMARY_PATH}" >/dev/null; then
    local kind rerun
    kind="$(jq -r '.blockers[0].kind' "${SUMMARY_PATH}")"
    rerun="$(jq -r '.blockers[0].rerun_command' "${SUMMARY_PATH}")"
    echo "LOCAL STATE BLOCKER: ${kind}; rerun after repair: ${rerun}"
    return
  fi
  if [ "${MODE}" = "verify" ] && jq -e '.repairable | length > 0' "${SUMMARY_PATH}" >/dev/null; then
    local kind repair
    kind="$(jq -r '.repairable[0].reason' "${SUMMARY_PATH}")"
    repair="$(jq -r '.repairable[0].repair_command' "${SUMMARY_PATH}")"
    echo "LOCAL STATE REPAIR REQUIRED: ${kind}; run: ${repair}"
    return
  fi
  local release runner
  release="$(jq -r '.release_posture' "${SUMMARY_PATH}")"
  runner="$(jq -r '.runner_posture.live_runner_required' "${SUMMARY_PATH}")"
  echo "local state ok: ${release}; live_runner_required=${runner}"
}

classify_listeners
classify_processes
classify_remotes
classify_source_roots
classify_runtime_observations
write_summary
print_text_summary

if jq -e '.blockers | length > 0' "${SUMMARY_PATH}" >/dev/null; then
  exit 1
fi
if [ "${MODE}" = "verify" ] && jq -e '.repairable | length > 0' "${SUMMARY_PATH}" >/dev/null; then
  exit 1
fi
exit 0
