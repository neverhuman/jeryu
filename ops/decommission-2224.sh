#!/usr/bin/env bash
#
# decommission-2224.sh — stop the retired GitLab forge that listens on :2224.
#
# The old jeryu server is a Docker Compose stack (project "jeryu", config
# /home/ubuntu/.jeryu/docker-compose.yml) whose `jeryu-gitlab` container
# publishes :2224 (git-over-SSH) and :8929 (GitLab HTTP). It is the last
# blocker for the fleet cutover and the authentic v4.0.0 release gate
# (ops/ci/verify-jeryu-env.sh fails closed while a retired provider listens).
#
# This script is conservative by design:
#   * It targets ONLY the "jeryu" compose project / the jeryu-gitlab container.
#     It NEVER touches any other project (e.g. nht-local-full) or container.
#   * It is a DRY RUN by default. Pass --yes to actually stop the stack.
#   * It keeps volumes by default (data is preserved, fully reversible with
#     `docker compose ... up -d`). Pass --purge to also remove volumes.
#
# Usage:
#   sudo bash ops/decommission-2224.sh            # dry run: show what it would do
#   sudo bash ops/decommission-2224.sh --yes      # stop + remove the stack (keep volumes)
#   sudo bash ops/decommission-2224.sh --yes --purge   # also remove volumes (irreversible)
#
set -euo pipefail

COMPOSE_FILE="/home/ubuntu/.jeryu/docker-compose.yml"
PROJECT="jeryu"
CONTAINER="jeryu-gitlab"
PORTS=(2224 8929)

APPLY=0
PURGE=0
for arg in "$@"; do
  case "$arg" in
    --yes) APPLY=1 ;;
    --purge) PURGE=1 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

note() { printf '\033[1;36m▶ %s\033[0m\n' "$*"; }
ok()   { printf '\033[32m✓ %s\033[0m\n' "$*"; }
warn() { printf '\033[33m! %s\033[0m\n' "$*"; }

command -v docker >/dev/null 2>&1 || { echo "docker not found on PATH" >&2; exit 1; }

listeners() { ss -ltnp 2>/dev/null | grep -E ":($(IFS='|'; echo "${PORTS[*]}"))\b" || true; }

note "retired-forge teardown — project '${PROJECT}', container '${CONTAINER}'"
echo "current listeners on ports ${PORTS[*]}:"
listeners | sed 's/^/    /' || true

# Confirm the container we are about to stop is the expected jeryu GitLab stack,
# and nothing else. We match by name AND by the published :2224 mapping.
target_id="$(docker ps --filter "name=^/${CONTAINER}$" --filter "publish=2224" -q 2>/dev/null || true)"
if [ -z "${target_id}" ]; then
  # fall back to "any container publishing 2224", but require its compose project to be PROJECT
  cand="$(docker ps --filter "publish=2224" -q 2>/dev/null || true)"
  if [ -n "${cand}" ]; then
    proj="$(docker inspect -f '{{ index .Config.Labels "com.docker.compose.project" }}' "${cand}" 2>/dev/null || true)"
    if [ "${proj}" = "${PROJECT}" ]; then
      target_id="${cand}"
    else
      warn "a container publishes :2224 but its compose project is '${proj}', NOT '${PROJECT}'."
      warn "refusing to touch it. Inspect manually: docker inspect ${cand}"
      exit 1
    fi
  fi
fi

if [ -z "${target_id}" ]; then
  ok "no running container publishes :2224 — the retired forge appears already stopped."
  if [ -z "$(listeners)" ]; then ok "no listeners on ${PORTS[*]} — nothing to do."; fi
  exit 0
fi

echo "would stop container: ${target_id} ($(docker inspect -f '{{.Name}}' "${target_id}" 2>/dev/null | sed 's#^/##'))"

down_cmd=(docker compose -p "${PROJECT}")
[ -f "${COMPOSE_FILE}" ] && down_cmd=(docker compose -f "${COMPOSE_FILE}" -p "${PROJECT}")
[ "${PURGE}" -eq 1 ] && DOWN_ARGS=(down -v) || DOWN_ARGS=(down)

if [ "${APPLY}" -ne 1 ]; then
  warn "DRY RUN — no changes made. Re-run with --yes to execute:"
  echo "    ${down_cmd[*]} ${DOWN_ARGS[*]}"
  [ "${PURGE}" -eq 0 ] && echo "    (volumes preserved; add --purge to remove them)"
  exit 0
fi

note "stopping the '${PROJECT}' GitLab stack: ${down_cmd[*]} ${DOWN_ARGS[*]}"
"${down_cmd[@]}" "${DOWN_ARGS[@]}"

# Verify the ports are gone.
sleep 2
remaining="$(listeners)"
if [ -n "${remaining}" ]; then
  warn "ports still listening after compose down:"
  echo "${remaining}" | sed 's/^/    /'
  warn "a stray process may hold the port; inspect with: ss -ltnp | grep -E ':(2224|8929)'"
  exit 1
fi
ok "retired forge stopped — no listeners on ${PORTS[*]}."
ok "fleet cutover + the authentic v4.0.0 release gate are now unblocked."
[ "${PURGE}" -eq 0 ] && echo "    (volumes kept; restore anytime with: ${down_cmd[*]} up -d)"
