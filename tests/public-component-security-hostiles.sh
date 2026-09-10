#!/usr/bin/env bash
# Synthetic command/lock contracts only; never executes Cargo, an auditor or network.
set -euo pipefail
(( $# <= 2 )) || exit 2
root=${1:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)}
packet=${2:-}
source "$root/tests/scratch.sh"
umask 077
temporary=$(mktemp -d "${TMPDIR:-/tmp}/jeryu-public-security.XXXXXXXX")
jeryu_record_test_scratch "$temporary"
cleanup() {
  local status=$?
  if (( status != 0 )); then
    printf 'Retaining failed public security fixtures: %s\n' "$temporary" >&2
    return "$status"
  fi
  jeryu_remove_test_scratch
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
source_path() {
  if [[ -n $packet ]]; then printf '%s/%s.after.sh\n' "$packet" "$1";
  else printf '%s/%s\n' "$root" "$2"; fi
}
helper=$(source_path 11 ops/ci/public-dependency-sources.sh)
passed=0
fail() { printf 'public component security control failed: %s\n' "$*" >&2; exit 1; }

# Exercise each exact new selection and final held-file guard, using the existing
# scope/receipt seam. No copied product checkout or Python evidence producer runs.
for specification in '01 jeryu-core tools/security-lane.sh' '02 jeryu-ci-runner tools/security-lane.sh' \
  '03 jeryu-deploy tools/security-lane.sh' '04 jeryu-jira ops/ci/security.sh'; do
  read -r number owner relative <<< "$specification"
  script=$(source_path "$number" "components/$owner/$relative")
  selection=$temporary/$number-selection.sh
  final=$temporary/$number-final.sh
  deny_loop=$temporary/$number-deny.sh
  awk '/^# Public candidate security consumes/{copy=1} copy{print} /^cargo_lock_before=/{exit}' "$script" > "$selection"
  awk '/^\[\[ -f \$cargo_lock_path/{block="";copy=1} copy{block=block $0 "\n"} copy && /^}$/{last=block;copy=0} END{printf "%s",last}' "$script" > "$final"
  awk '/^  # Each owning root retains/{copy=1} copy{print} copy && /^  done$/{exit}' "$script" > "$deny_loop"
  [[ -s $selection && -s $final ]] || fail 'missing actual lock guards'
  scope_source=$root/components/$owner/ops/ci/cargo-scope.sh
  if [[ $owner == jeryu-deploy ]]; then
    if [[ -n $packet ]]; then scope_source=$packet/14.after.sh; fi
  fi
  expected=$(awk -F"'" '/--argjson expected /{print $2}' "$scope_source")
  [[ $(jq -er 'length > 0' <<< "$expected") == true ]] || fail 'missing owning manifest inventory'
  modes=(standalone candidate bad-scope denied-receipt missing symlink hardlink mutated replaced chmod)
  [[ $owner == jeryu-jira ]] || modes+=(deny-failure)
  for mode in "${modes[@]}"; do
    directory=$temporary/$number-$mode
    repository=$directory/repository
    component=$repository/components/$owner
    mkdir -p "$component/ops/ci"
    while IFS= read -r manifest; do
      mkdir -p -- "$(dirname -- "$component/$manifest")"
      printf '[package]\n' > "$component/$manifest"
    done < <(jq -r '.[].[0]' <<< "$expected")
    jq -n --arg root "$component" --argjson expected "$expected" '
      {packages:[$expected[]|{name:.[1],manifest_path:($root+"/"+.[0])}]}
      | .packages[0].dependencies = [{name:.packages[-1].name,kind:null}]
    ' > "$repository/metadata.json"
    printf 'version = 4\n' > "$repository/Cargo.lock"
    printf 'component lock\n' > "$component/Cargo.lock"
    printf '[workspace]\n' > "$component/Cargo.toml"
    if [[ $mode == standalone ]]; then selected=$component/Cargo.lock; candidate=0;
    else selected=$repository/Cargo.lock; candidate=1; fi
    case $mode in
      missing) rm -- "$selected" ;;
      symlink) mv -- "$selected" "$directory/lock-original"; ln -s "$directory/lock-original" "$selected" ;;
      hardlink) ln -- "$selected" "$directory/lock-alias" ;;
    esac
    cat > "$component/ops/ci/cargo-scope.sh" <<'SCOPE'
component_root=$PWD
git_root=$EXPECTED_ROOT
[[ $CONTROL_MODE != bad-scope ]] || git_root=$EXPECTED_ROOT/other
cargo_metadata=$(cat "$EXPECTED_ROOT/metadata.json")
mapfile -t owned_packages < <(jq -r '.packages | sort_by(.name) | .[].name' <<< "$cargo_metadata")
SCOPE
    cat > "$component/ops/ci/workspace-lock.sh" <<'LOCK'
jeryu_deploy_record_workspace_lock() { [[ $CONTROL_MODE != bad-scope ]]; }
LOCK
    # Deploy's actual root-discovery call uses real Git, in this tiny fixture only.
    git -c core.hooksPath=/dev/null init --quiet --initial-branch=main "$repository"
    : > "$directory/deny-actual"
    if [[ $owner != jeryu-jira ]]; then
      [[ -s $deny_loop ]] || fail 'missing actual per-member deny loop'
      if [[ $mode == standalone ]]; then printf '%s\n' "$component/Cargo.toml" > "$directory/deny-expected";
      else jq -r '.packages | sort_by(.name) | .[].manifest_path' "$repository/metadata.json" > "$directory/deny-expected"; fi
    fi
    observed=0
    env CONTROL_MODE="$mode" EXPECTED_ROOT="$repository" ROOT="$component" \
      JERYU_MONOREPO_CANDIDATE="$candidate" SELECTION="$selection" FINAL="$final" \
      SELECTED="$selected" CASE_DIRECTORY="$directory" DENY_LOOP="$deny_loop" /bin/bash -euc '
      cd "$ROOT"
      require_jankurai() { [[ $CONTROL_MODE != denied-receipt ]]; }
      source "$SELECTION"
      [[ $cargo_lock_path == "$SELECTED" ]] || exit 91
      cargo() {
        [[ $# == 11 && $1 == deny && $2 == --locked && $3 == --manifest-path &&
           $5 == check && $6 == --config && $7 == "$ROOT/deny.toml" &&
           $8 == advisories && $9 == bans && ${10} == licenses && ${11} == sources ]] || exit 93
        printf "%s\n" "$4" >> "$CASE_DIRECTORY/deny-actual"
        [[ $CONTROL_MODE != deny-failure || $(wc -l < "$CASE_DIRECTORY/deny-actual") != 2 ]] || return 17
      }
      source "$DENY_LOOP"
      case $CONTROL_MODE in
        mutated) echo changed >> "$cargo_lock_path" ;;
        replaced) mv -- "$cargo_lock_path" "$CASE_DIRECTORY/previous"; cp -- "$CASE_DIRECTORY/previous" "$cargo_lock_path" ;;
        chmod) chmod 0644 -- "$cargo_lock_path" ;;
      esac
      source "$FINAL"
    ' > "$directory/output" 2>&1 || observed=$?
    case $mode in standalone|candidate)
      [[ $observed == 0 ]] || fail "$owner/$mode refused"
      if [[ $owner != jeryu-jira ]]; then
        cmp -s "$directory/deny-actual" "$directory/deny-expected" || fail "$owner owning roots or shared dependency excluded"
      fi ;;
      deny-failure) [[ $observed == 17 && $(wc -l < "$directory/deny-actual") == 2 ]] || fail "$owner deny failure did not stop" ;;
      *) [[ $observed != 0 ]] || fail "$owner/$mode accepted" ;;
    esac
    passed=$((passed + 1))
  done
done

for specification in '01 jeryu-core tools/security-lane.sh' '02 jeryu-ci-runner tools/security-lane.sh' \
  '03 jeryu-deploy tools/security-lane.sh' '04 jeryu-jira ops/ci/security.sh'; do
  read -r number owner relative <<< "$specification"
  script=$(source_path "$number" "components/$owner/$relative")
  audit_command=$(sed -n 's/^[[:space:]]*\(if \)\{0,1\}\(cargo audit --deny warnings.*\)$/\2/p' "$script")
  audit_command=${audit_command%; then}
  [[ -n $audit_command && $audit_command != *$'\n'* ]] || fail 'missing exact audit invocation'
  observed=$({
    printf '%s\n' 'cargo() { printf "%s\n" "$*"; }' "$audit_command"
  } | ROOT=/fixture/component cargo_lock_path=/fixture/root/Cargo.lock /bin/bash -eu)
  [[ $observed == 'audit --deny warnings --file /fixture/root/Cargo.lock' ]] || fail "$owner selected audit lock"
  passed=$((passed + 1))
done

mkdir "$temporary/bin"
cat > "$temporary/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
set -euo pipefail
[[ $GIT_CONFIG_GLOBAL == /dev/null && $GIT_CONFIG_SYSTEM == /dev/null &&
   $GIT_CONFIG_NOSYSTEM == 1 && $GIT_NO_REPLACE_OBJECTS == 1 &&
   $GIT_CONFIG_COUNT == 1 && $GIT_CONFIG_KEY_0 == credential.helper &&
   -z $GIT_CONFIG_VALUE_0 && -z ${GIT_CONFIG_KEY_9:-} && -z ${GIT_TRACE:-} &&
   -z ${GIT_DIR:-} ]] || exit 88
case "$*" in
  'run --locked --offline --quiet -p jeryu-split-tool --bin jeryu-split -- monorepo-check') stage=1 ;;
  "deny --locked --offline --all-features --manifest-path $PWD/Cargo.toml check sources --config $PWD/deny.toml") stage=2 ;;
  'run --locked --offline --quiet -p jeryu-split-tool --bin jeryu-split -- public-preflight') stage=3 ;;
  'test --locked --offline -p jeryu-runnerd --test hosted_dependency_transport -- --test-threads=1') stage=4 ;;
  'test --locked --offline -p jeryu-api --features web --test hosted_dependency_transport -- --test-threads=1') stage=4 ;;
  *) exit 89 ;;
esac
printf '%s\n' "$stage" >> "$TRACE"
printf 'SYNTHETIC PASS text does not override the exit code\n'
[[ $CONTROL_MODE != fail-$stage ]] || exit 17
if [[ $stage == 4 && $CONTROL_MODE == change-lock ]]; then printf 'changed\n' >> Cargo.lock; fi
if [[ $stage == 4 && $CONTROL_MODE == change-policy ]]; then printf 'changed\n' >> deny.toml; fi
CARGO
chmod 0700 "$temporary/bin/cargo"
for owner in jeryu-ci-runner jeryu-deploy; do
  if [[ $owner == jeryu-ci-runner ]]; then number=07; else number=08; fi
  dispatcher=$(source_path "$number" "components/$owner/ops/ci/dependency-sources.sh")
  for mode in pass fail-1 fail-2 fail-3 fail-4 change-lock change-policy denied-receipt broker bad-mode installed; do
    directory=$temporary/$owner-$mode
    repository=$directory/repository
    mkdir -p "$repository/ops/ci" "$repository/components/$owner/ops/ci"
    install -m 0700 "$helper" "$repository/ops/ci/public-dependency-sources.sh"
    install -m 0700 "$dispatcher" "$repository/components/$owner/ops/ci/dependency-sources.sh"
    printf '%s\n' 'printf "installed transport\n" >> "$TRACE"; exit 33' \
      > "$repository/components/$owner/ops/ci/hosted-git-env.sh"
    printf '[workspace]\n' > "$repository/Cargo.toml"
    printf 'version = 4\n' > "$repository/Cargo.lock"
    printf '[sources]\nunknown-git="deny"\nallow-git=[]\n' > "$repository/deny.toml"
    printf 'components/*/target/\n' > "$repository/.gitignore"
    cat > "$repository/ops/ci/lib.sh" <<'LIB'
require_jankurai() { [[ $CONTROL_MODE != denied-receipt ]]; }
jeryu_candidate_parent_custody() { [[ -d $1 && ! -L $1 && $(realpath -e -- "$1") == "$1" ]]; }
LIB
    git -c core.hooksPath=/dev/null init --quiet --initial-branch=main "$repository"
    git -C "$repository" -c core.hooksPath=/dev/null add .
    git -C "$repository" -c core.hooksPath=/dev/null -c user.name=Fixture -c user.email=fixture@example.invalid \
      -c commit.gpgsign=false commit --quiet -m fixture
    head=$(git -C "$repository" rev-parse HEAD)
    : > "$directory/trace"
    candidate=1 broker=0
    [[ $mode != bad-mode ]] || candidate=2
    [[ $mode != installed ]] || candidate=0
    [[ $mode != broker ]] || broker=1
    observed=0
    env -i PATH="$temporary/bin:/usr/bin:/bin" HOME="$temporary" \
      CONTROL_MODE="$mode" TRACE="$directory/trace" JERYU_MONOREPO_CANDIDATE="$candidate" \
      JERYU_MONOREPO_EXPECTED_HEAD="$head" JAIN_RELEASE_CI="$broker" \
      GIT_CONFIG_GLOBAL=/private/forbidden GIT_CONFIG_COUNT=10 GIT_CONFIG_KEY_9=url.private.insteadOf \
      GIT_TRACE=/private/forbidden GIT_DIR=/private/forbidden \
      /bin/bash "$repository/components/$owner/ops/ci/dependency-sources.sh" \
      > "$directory/output" 2>&1 || observed=$?
    mapfile -t receipts < <(find "$repository/components/$owner" -name evidence.json -type f)
    if [[ $mode == pass ]]; then
      [[ $observed == 0 && ${#receipts[@]} == 1 && $(<"$directory/trace") == $'1\n2\n3\n4' ]] || fail "$owner/$mode dispatch"
      jq -e --arg component "$owner" --arg head "$head" '
        .component == $component and .git.head == $head and .conclusion == "success" and
        .installed_authority == false and .public_origin_build == false and
        (.commands | length) == 4 and all(.commands[]; .exit_status == 0)
      ' "${receipts[0]}" >/dev/null || fail "$owner/$mode receipt"
    else
      [[ $observed != 0 && ${#receipts[@]} == 0 ]] || fail "$owner/$mode false success"
      if [[ $mode == installed ]]; then
        [[ $observed == 33 && $(<"$directory/trace") == 'installed transport' ]] || fail "$owner installed authority changed"
      fi
      if [[ $mode == fail-* ]]; then
        stage=${mode#fail-}
        [[ $observed == 17 && $(wc -l < "$directory/trace") == "$stage" ]] || fail "$owner/$mode status/order"
        event=$(find "$repository/components/$owner" -name commands.jsonl -type f)
        jq -se --argjson count "$stage" 'length == $count and .[-1].exit_status == 17' "$event" >/dev/null || fail 'missing failed command receipt'
      fi
    fi
    passed=$((passed + 1))
  done
done
jeryu_remove_test_scratch
trap - EXIT
printf 'public component security: %s synthetic controls passed; no real Cargo or auditor executed\n' "$passed"
