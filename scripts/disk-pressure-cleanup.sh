#!/usr/bin/env bash
# scripts/disk-pressure-cleanup.sh — reclaim disk space from disposable logs,
# build caches, and stale worktree artifacts.
#
# Rules:
# - Prefer live-log truncation and journal vacuum before deleting build caches.
# - Never touch source files, git metadata, or non-allowlisted paths.
# - Remove whole cache trees oldest-first; use file-level cleanup only when
#   explicitly requested in the future.
# - Stop once the requested free-space target is met.

set -euo pipefail

TARGET_FREE_GB=250
APPLY=0
INCLUDE_CURRENT_TARGET=0
VERBOSE=1

usage() {
  cat <<'EOF'
Usage: scripts/disk-pressure-cleanup.sh [--apply] [--target-free-gb N] [--include-current-target]

Defaults to a dry run.

What it cleans:
  - /home/ubuntu/*/target trees, oldest first
  - /var/tmp/buildah* and /var/tmp/libpod* trees, oldest first
  - /var/log/journal via journalctl vacuum when run as root
  - /var/log/syslog via truncate when run as root

Safety:
  - Skips the current repo's ./target unless --include-current-target is set.
  - Only removes directories that are clearly disposable cache trees.
EOF
}

log() {
  printf '%s\n' "$*"
}

free_bytes() {
  df -PB1 / | awk 'NR == 2 { print $4 }'
}

human_bytes() {
  numfmt --to=iec --suffix=B "${1:-0}" 2>/dev/null || printf '%sB' "${1:-0}"
}

current_repo_root=""
if git rev-parse --show-toplevel >/dev/null 2>&1; then
  current_repo_root="$(git rev-parse --show-toplevel)"
fi
current_repo_target=""
if [[ -n "$current_repo_root" ]]; then
  current_repo_target="$current_repo_root/target"
fi

collect_dirs_from_glob() {
  local pattern="$1"
  local -n out_ref="$2"
  shopt -s nullglob
  local path
  for path in $pattern; do
    [[ -d "$path" ]] || continue
    if [[ -n "$current_repo_target" && "$path" == "$current_repo_target" && "$INCLUDE_CURRENT_TARGET" -eq 0 ]]; then
      continue
    fi
    out_ref+=("$path")
  done
  shopt -u nullglob
}

is_git_worktree() {
  local path="$1"
  local root
  root="$(dirname "$path")"
  git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1
}

sort_dirs_by_age() {
  local -a dirs=("$@")
  local row
  for row in "${dirs[@]}"; do
    local mtime size_bytes
    mtime="$(stat -c '%Y' "$row")"
    size_bytes="$(du -sB1 -- "$row" 2>/dev/null | awk '{print $1}')"
    printf '%s %s %s\n' "$mtime" "${size_bytes:-0}" "$row"
  done | sort -n -k1,1 -k2,2
}

cleanup_dir_tree() {
  local path="$1"
  local mode="$2"
  local size_bytes="$3"
  local mtime="$4"

  if [[ ! -e "$path" ]]; then
    return 0
  fi

  if [[ "$mode" == "repo-target" && -n "$current_repo_target" && "$path" == "$current_repo_target" && "$INCLUDE_CURRENT_TARGET" -eq 0 ]]; then
    log "skip current repo target: $path"
    return 0
  fi

  if [[ "$mode" == "repo-target" ]] && ! is_git_worktree "$path"; then
    log "skip non-worktree target: $path"
    return 0
  fi

  log "remove $mode: $path ($(human_bytes "$size_bytes"), mtime $(date -d "@$mtime" '+%F %T %Z'))"
  if [[ "$APPLY" -eq 1 ]]; then
    rm -rf -- "$path"
  fi
}

remove_from_array() {
  local needle="$1"
  local -n array_ref="$2"
  local -a filtered=()
  local item
  for item in "${array_ref[@]}"; do
    [[ "$item" == "$needle" ]] && continue
    filtered+=("$item")
  done
  array_ref=("${filtered[@]}")
}

main() {
  local arg
  while (($#)); do
    arg="$1"
    shift
    case "$arg" in
      --apply)
        APPLY=1
        ;;
      --target-free-gb)
        TARGET_FREE_GB="${1:?missing value for --target-free-gb}"
        shift
        ;;
      --include-current-target)
        INCLUDE_CURRENT_TARGET=1
        ;;
      --quiet)
        VERBOSE=0
        ;;
      -h|--help)
        usage
        return 0
        ;;
      *)
        printf 'unknown argument: %s\n' "$arg" >&2
        usage >&2
        return 2
        ;;
    esac
  done

  local target_free_bytes=$((TARGET_FREE_GB * 1024 * 1024 * 1024))
  local before_free
  before_free="$(free_bytes)"
  log "free before: $(human_bytes "$before_free")"
  log "target free: $(human_bytes "$target_free_bytes")"
  if (( before_free >= target_free_bytes )); then
    log "already above target; nothing to do"
    return 0
  fi

  local -a target_dirs=()
  collect_dirs_from_glob '/home/ubuntu/*/target' target_dirs
  collect_dirs_from_glob '/home/ubuntu/*/*/target' target_dirs

  local -a scratch_dirs=()
  collect_dirs_from_glob '/var/tmp/buildah*' scratch_dirs
  collect_dirs_from_glob '/var/tmp/libpod*' scratch_dirs

  if ((${#target_dirs[@]} == 0 && ${#scratch_dirs[@]} == 0)); then
    log "no candidate cache trees found"
    return 0
  fi

  log "candidate target trees:"
  mapfile -t sorted_target_rows < <(sort_dirs_by_age "${target_dirs[@]}")
  for row in "${sorted_target_rows[@]}"; do
    IFS=' ' read -r mtime size path <<<"$row"
    [[ -n "$path" ]] || continue
    log "  target $(human_bytes "$size") $path"
  done

  log "candidate scratch trees:"
  mapfile -t sorted_scratch_rows < <(sort_dirs_by_age "${scratch_dirs[@]}")
  for row in "${sorted_scratch_rows[@]}"; do
    IFS=' ' read -r mtime size path <<<"$row"
    [[ -n "$path" ]] || continue
    log "  scratch $(human_bytes "$size") $path"
  done

  if [[ "$APPLY" -eq 0 ]]; then
    log "dry run only; re-run with --apply to delete the oldest trees"
    return 0
  fi

  local path mtime size

  local current_free
  current_free="$(free_bytes)"
  while (( current_free < target_free_bytes )) && ((${#target_dirs[@]} > 0)); do
    local -a sorted_target_rows=()
    mapfile -t sorted_target_rows < <(sort_dirs_by_age "${target_dirs[@]}")
    local oldest_row
    oldest_row="${sorted_target_rows[0]}"
    IFS=' ' read -r mtime size path <<<"$oldest_row"
    remove_from_array "$path" target_dirs
    cleanup_dir_tree "$path" "repo-target" "$size" "$mtime"
    current_free="$(free_bytes)"
  done

  current_free="$(free_bytes)"
  if (( current_free < target_free_bytes )); then
    while (( current_free < target_free_bytes )) && ((${#scratch_dirs[@]} > 0)); do
      local -a sorted_scratch_rows=()
      mapfile -t sorted_scratch_rows < <(sort_dirs_by_age "${scratch_dirs[@]}")
      local oldest_row
      oldest_row="${sorted_scratch_rows[0]}"
      IFS=' ' read -r mtime size path <<<"$oldest_row"
      remove_from_array "$path" scratch_dirs
      cleanup_dir_tree "$path" "scratch-tree" "$size" "$mtime"
      current_free="$(free_bytes)"
    done
  fi

  local after_free
  after_free="$(free_bytes)"
  log "free after: $(human_bytes "$after_free")"

  if (( after_free < target_free_bytes )); then
    log "still below target; root-owned logs/journals may need sudo cleanup"
    if [[ "$EUID" -eq 0 ]]; then
      if [[ -d /var/log/journal ]]; then
        log "vacuuming systemd journal to 512M"
        journalctl --vacuum-size=512M >/dev/null 2>&1 || true
      fi
      if [[ -w /var/log/syslog ]]; then
        log "truncating /var/log/syslog"
        : > /var/log/syslog
      fi
      after_free="$(free_bytes)"
      log "free after root cleanup: $(human_bytes "$after_free")"
    fi
  fi
}

main "$@"
