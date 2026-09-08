#!/usr/bin/env bash
# Replacement-blind, ambient-environment-independent source authority shared by
# every evidence-producing lane. This file is sourced; callers provide repo_root.
set -euo pipefail

jeryu_source_fail() {
  printf 'source authority failed: %s\n' "$*" >&2
  return 1
}

jeryu_governed_git() {
  [[ -n "${repo_root:-}" ]] || {
    jeryu_source_fail 'repo_root is not set'
    return 1
  }
  /usr/bin/env -i \
    HOME=/nonexistent PATH=/usr/bin:/bin LANG=C LC_ALL=C \
    GIT_ATTR_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    GIT_NO_REPLACE_OBJECTS=1 GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 \
    GIT_CONFIG_COUNT=4 \
    GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0="$repo_root" \
    GIT_CONFIG_KEY_1=core.fsmonitor GIT_CONFIG_VALUE_1=false \
    GIT_CONFIG_KEY_2=core.hooksPath GIT_CONFIG_VALUE_2=/dev/null \
    GIT_CONFIG_KEY_3=core.attributesFile GIT_CONFIG_VALUE_3=/dev/null \
    /usr/bin/git --no-replace-objects -C "$repo_root" "$@"
}

# Run a non-Git tool that may spawn Git (for example gitleaks) without letting
# caller Git discovery, object, index, replacement, or config authority leak in.
jeryu_with_scrubbed_git() (
  local name
  while IFS='=' read -r name _; do
    case "$name" in
      GIT_DIR|GIT_WORK_TREE|GIT_INDEX_FILE|GIT_OBJECT_DIRECTORY|\
      GIT_ALTERNATE_OBJECT_DIRECTORIES|GIT_COMMON_DIR|GIT_NAMESPACE|\
      GIT_REPLACE_REF_BASE|GIT_SHALLOW_FILE|GIT_GRAFT_FILE|\
      GIT_CEILING_DIRECTORIES|GIT_DISCOVERY_ACROSS_FILESYSTEM|\
      GIT_CONFIG|GIT_CONFIG_GLOBAL|GIT_CONFIG_SYSTEM|GIT_CONFIG_NOSYSTEM|\
      GIT_CONFIG_COUNT|GIT_CONFIG_KEY_*|GIT_CONFIG_VALUE_*)
        unset "$name"
        ;;
    esac
  done < <(/usr/bin/env)
  export PATH=/usr/bin:/bin
  export GIT_ATTR_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
  export GIT_NO_REPLACE_OBJECTS=1 GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0
  export GIT_CONFIG_COUNT=2
  export GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0="$repo_root"
  export GIT_CONFIG_KEY_1=core.fsmonitor GIT_CONFIG_VALUE_1=false
  exec "$@"
)

jeryu_tracked_inputs_sha256() {
  local path physical mode digest
  {
    while IFS= read -r -d '' path; do
      [[ "$path" != *$'\n'* && "$path" != *$'\r'* ]] ||
        jeryu_source_fail 'tracked path contains a line break'
      physical="$repo_root/$path"
      [[ -f "$physical" && ! -L "$physical" ]] || {
        jeryu_source_fail "tracked source is not a physical regular file: $path"
        return 1
      }
      [[ "$(stat -c '%h' -- "$physical")" == 1 ]] || {
        jeryu_source_fail "tracked source is multiply linked: $path"
        return 1
      }
      mode="$(stat -c '%a' -- "$physical")" || return 1
      digest="$(sha256sum -- "$physical" | awk '{print $1}')" || return 1
      printf '%s\0%s\0%s\0' "$path" "$mode" "$digest"
    done < <(jeryu_governed_git ls-files -z)
  } | sha256sum | awk '{print $1}'
}

jeryu_require_physical_source() {
  local top git_dir common_dir objects record ignored worktree_count hidden_error=0 ignored_error=0
  [[ "${repo_root:-}" == /* && -d "$repo_root" && ! -L "$repo_root" ]] || {
    jeryu_source_fail 'repo_root is not a physical absolute directory'
    return 1
  }
  [[ "$(realpath -e -- "$repo_root")" == "$repo_root" ]] || {
    jeryu_source_fail 'repo_root is not its physical canonical path'
    return 1
  }
  [[ -x /usr/bin/git && ! -L /usr/bin/git ]] || {
    jeryu_source_fail '/usr/bin/git is not a physical executable'
    return 1
  }
  [[ -d "$repo_root/.git" && ! -L "$repo_root/.git" ]] || {
    jeryu_source_fail 'canonical .git is not a physical directory'
    return 1
  }

  top="$(jeryu_governed_git rev-parse --show-toplevel)" || return 1
  git_dir="$(jeryu_governed_git rev-parse --path-format=absolute --absolute-git-dir)" || return 1
  common_dir="$(jeryu_governed_git rev-parse --path-format=absolute --git-common-dir)" || return 1
  objects="$(jeryu_governed_git rev-parse --path-format=absolute --git-path objects)" || return 1
  [[ "$top" == "$repo_root" && "$git_dir" == "$repo_root/.git" &&
     "$common_dir" == "$repo_root/.git" && "$objects" == "$repo_root/.git/objects" ]] ||
    { jeryu_source_fail 'Git top-level, gitdir, common-dir, or object custody differs from canonical'; return 1; }

  [[ ! -e "$repo_root/.git/objects/info/alternates" &&
     ! -L "$repo_root/.git/objects/info/alternates" ]] ||
    { jeryu_source_fail 'repository object alternates are forbidden'; return 1; }
  [[ ! -e "$repo_root/.git/info/grafts" && ! -L "$repo_root/.git/info/grafts" ]] ||
    { jeryu_source_fail 'repository grafts are forbidden'; return 1; }
  [[ -z "$(jeryu_governed_git for-each-ref --format='%(refname)' refs/replace)" ]] ||
    { jeryu_source_fail 'replacement refs are forbidden'; return 1; }
  [[ -z "$(jeryu_governed_git config --local --get core.worktree || true)" ]] ||
    { jeryu_source_fail 'local core.worktree override is forbidden'; return 1; }

  worktree_count="$(jeryu_governed_git worktree list --porcelain | awk '$1 == "worktree" {count++} END {print count+0}')"
  [[ "$worktree_count" == 1 ]] || {
    jeryu_source_fail 'exactly one registered checkout is required'
    return 1
  }

  while IFS= read -r -d '' record; do
    case "${record:0:1}" in
      [a-zS])
        printf 'source authority failed: hidden index flag is forbidden: %s\n' \
          "${record:2}" >&2
        hidden_error=1
        ;;
    esac
  done < <(jeryu_governed_git ls-files -v -z)
  [[ "$hidden_error" == 0 ]] || return 1

  jeryu_governed_git diff-index --cached --quiet HEAD -- ||
    { jeryu_source_fail 'index differs from literal HEAD'; return 1; }
  jeryu_governed_git diff-files --quiet -- ||
    { jeryu_source_fail 'tracked worktree bytes differ from the index'; return 1; }
  [[ -z "$(jeryu_governed_git status --porcelain=v1 --untracked-files=all)" ]] ||
    { jeryu_source_fail 'source checkout is not clean'; return 1; }

  while IFS= read -r -d '' ignored; do
    case "$ignored" in
      target/*|dossiers/*|.jankurai/repo-score.json|.jankurai/repo-score.md|\
      .jankurai/score-history.jsonl) ;;
      *)
        printf 'source authority failed: ignored input outside a derived zone is forbidden: %s\n' \
          "$ignored" >&2
        ignored_error=1
        ;;
    esac
  done < <(jeryu_governed_git ls-files --others --ignored --exclude-standard -z)
  [[ "$ignored_error" == 0 ]] || return 1
}

jeryu_source_snapshot() {
  jeryu_require_physical_source || return 1
  JERYU_SOURCE_HEAD="$(jeryu_governed_git rev-parse --verify 'HEAD^{commit}')" || return 1
  JERYU_SOURCE_TREE="$(jeryu_governed_git rev-parse --verify 'HEAD^{tree}')" || return 1
  JERYU_SOURCE_INPUTS_SHA256="$(jeryu_tracked_inputs_sha256)" || return 1
  [[ "$JERYU_SOURCE_HEAD" =~ ^[0-9a-f]{40}$ &&
     "$JERYU_SOURCE_TREE" =~ ^[0-9a-f]{40}$ &&
     "$JERYU_SOURCE_INPUTS_SHA256" =~ ^[0-9a-f]{64}$ ]] ||
    { jeryu_source_fail 'source identity is malformed'; return 1; }
  export JERYU_SOURCE_HEAD JERYU_SOURCE_TREE JERYU_SOURCE_INPUTS_SHA256
}

jeryu_source_verify() {
  local expected_head="$1" expected_tree="$2" expected_inputs="$3"
  jeryu_require_physical_source || return 1
  [[ "$(jeryu_governed_git rev-parse --verify 'HEAD^{commit}')" == "$expected_head" ]] ||
    { jeryu_source_fail 'HEAD moved during proof'; return 1; }
  [[ "$(jeryu_governed_git rev-parse --verify 'HEAD^{tree}')" == "$expected_tree" ]] ||
    { jeryu_source_fail 'tree moved during proof'; return 1; }
  [[ "$(jeryu_tracked_inputs_sha256)" == "$expected_inputs" ]] ||
    { jeryu_source_fail 'physical tracked inputs moved during proof'; return 1; }
}

# Reject a Jankurai score report whose Git identity/toplevel does not match the
# bound physical source snapshot. Callers pass the report path and expected HEAD.
jeryu_require_score_report_matches_source() {
  local report_path="$1" expected_head="$2"
  local report_repo report_git_head report_git_dirty
  [[ -f "$report_path" && ! -L "$report_path" ]] ||
    { jeryu_source_fail 'score report is not a physical regular file'; return 1; }
  [[ "$expected_head" =~ ^[0-9a-f]{40}$ ]] ||
    { jeryu_source_fail 'expected source HEAD is malformed'; return 1; }
  report_repo="$(jq -er '.repo' "$report_path")" ||
    { jeryu_source_fail 'score report is missing repo identity'; return 1; }
  [[ "$report_repo" == "." ]] ||
    { jeryu_source_fail "score report repo identity is ${report_repo}, expected ."; return 1; }
  report_git_head="$(jq -er '.git.head' "$report_path")" ||
    { jeryu_source_fail 'score report is missing git.head'; return 1; }
  [[ "$report_git_head" =~ ^[0-9a-f]{7,40}$ ]] ||
    { jeryu_source_fail "score report git.head is malformed: ${report_git_head}"; return 1; }
  [[ "$expected_head" == "$report_git_head"* ]] ||
    { jeryu_source_fail "score report git.head ${report_git_head} does not match bound source ${expected_head}"; return 1; }
  report_git_dirty="$(jq -r 'if (.git|type)=="object" and (.git|has("dirty_worktree")) then .git.dirty_worktree elif has("dirty_worktree") then .dirty_worktree else empty end | tostring' "$report_path")" ||
    { jeryu_source_fail 'score report is missing dirty_worktree'; return 1; }
  [[ -n "$report_git_dirty" ]] ||
    { jeryu_source_fail 'score report is missing dirty_worktree'; return 1; }
  [[ "$report_git_dirty" == "false" ]] ||
    { jeryu_source_fail "score report dirty_worktree is ${report_git_dirty}, expected false"; return 1; }
}
