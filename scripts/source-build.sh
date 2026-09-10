#!/usr/bin/env bash
# Shared source identity for build and installation. Paths are NUL-delimited.
set -euo pipefail

source_digest() {
  (
    cd "$1"
    git ls-files -z --cached --others --exclude-standard |
      LC_ALL=C sort -zu |
      while IFS= read -r -d '' path; do
        if [[ -L "$path" ]]; then
          printf 'source input must not be a symlink: %s\n' "$path" >&2
          exit 1
        fi
        if [[ -f "$path" ]]; then sha256sum --zero -- "$path"; fi
      done |
      sha256sum | cut -d ' ' -f 1
  )
}

# Explicit split preparation uses this closed Git view, never personal routing.
split_source_git() {
  local directory=$1
  shift
  env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_NO_REPLACE_OBJECTS=1 \
    GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 \
    GIT_AUTHOR_NAME="Jeryu split verification" GIT_AUTHOR_EMAIL=split@jeryu.invalid \
    GIT_COMMITTER_NAME="Jeryu split verification" GIT_COMMITTER_EMAIL=split@jeryu.invalid \
    GIT_AUTHOR_DATE=2000-01-01T00:00:00Z GIT_COMMITTER_DATE=2000-01-01T00:00:00Z \
    /usr/bin/git -c core.fsmonitor=false -C "$directory" "$@"
}

# Optional component/tree bind a generated descriptor to its original subtree.
split_source_snapshot() (
  local directory=${1:-} revision=${2:-} component=${3:-} subtree=${4:-} status flags replacements record mode oid path observed before after permissions
  [[ $# == 2 || $# == 4 ]] || exit 1
  [[ $directory =~ ^/[A-Za-z0-9_./-]+$ && $revision =~ ^[0-9a-f]{40}$ &&
     -d $directory && ! -L $directory && -O $directory &&
     $(realpath -e -- "$directory") == "$directory" &&
     -d $directory/.git && ! -L $directory/.git &&
     ! -e $directory/.git/index.lock && ! -L $directory/.git/index.lock &&
     ! -e $directory/.git/objects/info/alternates && ! -L $directory/.git/objects/info/alternates &&
     ! -e $directory/.git/objects/info/http-alternates && ! -L $directory/.git/objects/info/http-alternates &&
     $(split_source_git "$directory" rev-parse --is-shallow-repository) == false &&
     $(split_source_git "$directory" rev-parse --show-toplevel) == "$directory" &&
     $(split_source_git "$directory" rev-parse HEAD) == "$revision" ]] || exit 1
  if [[ -n $component || -n $subtree ]]; then
    [[ $component =~ ^jeryu-[a-z-]+$ && $subtree =~ ^[0-9a-f]{40}$ &&
       $(split_source_git "$directory" rev-parse "HEAD:components/$component") == "$subtree" ]] || exit 1
  fi
  replacements=$(split_source_git "$directory" for-each-ref --format='%(refname)' refs/replace/) || exit 1
  [[ -z $replacements ]] || exit 1
  status=$(split_source_git "$directory" status --porcelain=v1 --untracked-files=all) || exit 1
  flags=$(split_source_git "$directory" ls-files -v) || exit 1
  [[ -z $status && -n $flags && ! $flags =~ (^|$'\n')[a-zS] ]] || exit 1
  # Reuse Web source admission: compare raw working bytes to committed blobs,
  # even when clean filters, stat caching or core.filemode hide changes from status.
  split_source_git "$directory" ls-tree -r -z "$revision" |
    while IFS= read -r -d '' record; do
      mode=${record%% *}; path=${record#*$'\t'}
      oid=${record%%$'\t'*}; oid=${oid##* }
      [[ $mode == 100644 || $mode == 100755 ]] || exit 1
      [[ -f $directory/$path && ! -L $directory/$path && -O $directory/$path &&
         $(realpath -e -- "$directory/$path") == "$directory/$path" &&
         $(stat -c %h -- "$directory/$path") == 1 ]] || exit 1
      before=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$directory/$path") || exit 1
      permissions=$(stat -c %a -- "$directory/$path") || exit 1
      if [[ $mode == 100755 ]]; then
        (( (8#$permissions & 8#100) != 0 )) || exit 1
      else
        (( (8#$permissions & 8#100) == 0 )) || exit 1
      fi
      observed=$(split_source_git "$directory" hash-object --no-filters -- "$directory/$path") || exit 1
      after=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$directory/$path") || exit 1
      [[ $observed == "$oid" && $before == "$after" && ! -L $directory/$path ]] || exit 1
    done || exit 1
  split_source_git "$directory" rev-parse HEAD 'HEAD^{tree}' || exit 1
  stat -c '%d:%i:%u:%g:%a' -- "$directory" || exit 1
  git() { split_source_git "$PWD" "$@"; }
  source_digest "$directory" || exit 1
)

# Cargo keeps the public SourceId. Only this command invocation transports the
# exact canonical URL to the explicitly admitted local repository. No mapping
# is written into a manifest, lock, exported tree or persistent Git config.
split_source_run() (
  [[ $# -ge 5 ]] || exit 2
  local directory=$1 revision=$2 component=$3 subtree=$4 before after status=0 variable
  local -a scrub=()
  shift 4
  before=$(split_source_snapshot "$directory" "$revision" "$component" "$subtree") || exit 1
  for variable in "${!GIT_@}" "${!SSH_@}"; do
    [[ -z $variable ]] || scrub+=(-u "$variable")
  done
  printf 'split transport: local-source-preparation commit=%s; public origin unproven\n' "$revision" >&2
  env "${scrub[@]}" GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null \
    GIT_CONFIG_NOSYSTEM=1 GIT_NO_REPLACE_OBJECTS=1 GIT_OPTIONAL_LOCKS=0 \
    GIT_TERMINAL_PROMPT=0 GIT_ALLOW_PROTOCOL=file:https GIT_CONFIG_COUNT=1 \
    "GIT_CONFIG_KEY_0=url.file://$directory.insteadOf" \
    GIT_CONFIG_VALUE_0=https://github.com/neverhuman/jeryu.git \
    CARGO_NET_GIT_FETCH_WITH_CLI=true "$@" || status=$?
  after=$(split_source_snapshot "$directory" "$revision" "$component" "$subtree") || exit 1
  [[ $after == "$before" ]] || exit 1
  exit "$status"
)
