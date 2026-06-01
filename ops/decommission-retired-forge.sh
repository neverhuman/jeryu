#!/usr/bin/env bash
#
# decommission-retired-forge.sh - stop the retired forge listeners managed by Docker.
#
# Usage:
#   sudo bash ops/decommission-retired-forge.sh
#   sudo bash ops/decommission-retired-forge.sh --yes
#   sudo bash ops/decommission-retired-forge.sh --yes --purge
#
# Dry run is the default. `--yes` removes matching Docker containers. `--purge`
# also removes anonymous volumes attached to those containers.

set -euo pipefail

TARGET_PORTS=(2224 8929)
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

listeners() {
  ss -ltnp 2>/dev/null | grep -E ":($(IFS='|'; echo "${TARGET_PORTS[*]}"))\b" || true
}

matching_containers() {
  local -A seen=()
  local port id
  for port in "${TARGET_PORTS[@]}"; do
    while IFS= read -r id; do
      [ -n "${id}" ] || continue
      seen["${id}"]=1
    done < <(docker ps -q --filter "publish=${port}" 2>/dev/null || true)
  done

  for id in "${!seen[@]}"; do
    printf '%s\n' "${id}"
  done
}

remove_containers() {
  local ids=("$@")

  if [ "${APPLY}" -ne 1 ]; then
    warn "DRY RUN — would remove ${#ids[@]} container(s) publishing the retired forge ports:"
    printf '    docker rm%s %s\n' \
      "$( [ "${PURGE}" -eq 1 ] && printf -- ' -v' )" \
      "${ids[*]}"
    return 0
  fi

  if [ "${PURGE}" -eq 1 ]; then
    note "removing ${#ids[@]} container(s) and anonymous volumes"
    docker rm -fv "${ids[@]}" >/dev/null 2>&1 || true
  else
    note "removing ${#ids[@]} container(s)"
    docker rm -f "${ids[@]}" >/dev/null 2>&1 || true
  fi
}

echo "current listeners on ports ${TARGET_PORTS[*]}:"
listeners | sed 's/^/    /' || true

mapfile -t ids < <(matching_containers)

if [ "${#ids[@]}" -eq 0 ]; then
  ok "no Docker containers publish the retired forge ports."
  if [ -z "$(listeners)" ]; then
    ok "no listeners on ${TARGET_PORTS[*]}."
  else
    warn "listeners remain on the retired forge ports; inspect with: ss -ltnp | grep -E ':(2224|8929)'"
  fi
  exit 0
fi

printf 'containers publishing the retired forge ports:\n'
printf '    %s\n' "${ids[@]}"

if [ "${APPLY}" -ne 1 ]; then
  remove_containers "${ids[@]}"
  exit 0
fi

remove_containers "${ids[@]}"

sleep 2
remaining="$(listeners)"
if [ -n "${remaining}" ]; then
  warn "ports still listening after container removal:"
  echo "${remaining}" | sed 's/^/    /'
  exit 1
fi

ok "retired forge stopped — no listeners on ${TARGET_PORTS[*]}."
[ "${PURGE}" -eq 0 ] && echo "    (anonymous volumes kept; re-run with --purge to remove them)"
