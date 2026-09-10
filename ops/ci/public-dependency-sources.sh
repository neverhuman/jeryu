#!/usr/bin/env bash
# Actual public monorepo dependency admission; installed split transport is separate.
set -euo pipefail
(( $# == 1 )) && [[ $1 == jeryu-ci-runner || $1 == jeryu-deploy ]] || {
  printf 'usage: public-dependency-sources.sh {jeryu-ci-runner|jeryu-deploy}\n' >&2; exit 2;
}
[[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 && ${JAIN_RELEASE_CI:-0} != 1 ]] || {
  printf 'public dependency proof requires an explicit non-broker candidate\n' >&2; exit 1;
}
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
component=$root/components/$1
[[ -d $component && ! -L $component && $(realpath -e -- "$component") == "$component" ]] || exit 1
# The existing verifier binds the actual auditor, receipt, clean commit and all consumers.
source "$root/ops/ci/lib.sh"
require_jankurai

public_command() (
  local name
  # Keep admitted compiler/tool PATH, but no Git discovery, trace, credential or URL overlay.
  while IFS= read -r name; do unset "$name" || exit 1; done < <(compgen -A variable GIT_ || true)
  export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1
  export GIT_NO_REPLACE_OBJECTS=1 GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0
  export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0=
  "$@"
)
cd "$root"
head=$(public_command /usr/bin/git rev-parse HEAD)
tree=$(public_command /usr/bin/git rev-parse 'HEAD^{tree}')
[[ $head == "$JERYU_MONOREPO_EXPECTED_HEAD" && $head =~ ^[0-9a-f]{40}$ && $tree =~ ^[0-9a-f]{40}$ ]] || exit 1
lock_before=$(sha256sum -- "$root/Cargo.lock")
policy_before=$(sha256sum -- "$root/deny.toml")

umask 077
for directory in "$component/target" "$component/target/security"; do
  [[ ! -L $directory ]] || exit 1
  if [[ ! -e $directory ]]; then mkdir -- "$directory"; fi
  jeryu_candidate_parent_custody "$directory"
done
attempt=$(mktemp -d "$component/target/security/public-dependencies.XXXXXXXX")
[[ $(stat -c %u:%a -- "$attempt") == "$(id -u):700" && ! -L $attempt ]] || exit 1
printf 'Public dependency attempt retained at %s\n' "$attempt" >&2
: > "$attempt/commands.jsonl"
step=0
record_command() {
  local status=0 digest
  step=$((step + 1))
  public_command "$@" > "$attempt/$step.log" 2>&1 || status=$?
  cat -- "$attempt/$step.log"
  digest=$(sha256sum -- "$attempt/$step.log")
  jq -nc --args --argjson exit_status "$status" --arg log_sha256 "${digest%% *}" \
    --argjson step "$step" '{step:$step,argv:$ARGS.positional,exit_status:$exit_status,log_sha256:$log_sha256}' \
    -- "$@" >> "$attempt/commands.jsonl"
  return "$status"
}
record_command cargo run --locked --offline --quiet -p jeryu-split-tool --bin jeryu-split -- monorepo-check
# Root SQLite source policy is stricter than historical split allowlists: no Git packages.
record_command cargo deny --locked --offline --all-features --manifest-path "$root/Cargo.toml" check sources --config "$root/deny.toml"
record_command cargo run --locked --offline --quiet -p jeryu-split-tool --bin jeryu-split -- public-preflight
# Retain each existing real transport regression command, including Runner receipt hostiles.
if [[ $1 == jeryu-ci-runner ]]; then
  record_command cargo test --locked --offline -p jeryu-runnerd --test hosted_dependency_transport -- --test-threads=1
else
  record_command cargo test --locked --offline -p jeryu-api --features web --test hosted_dependency_transport -- --test-threads=1
fi
require_jankurai
[[ $(sha256sum -- "$root/Cargo.lock") == "$lock_before" &&
   $(sha256sum -- "$root/deny.toml") == "$policy_before" &&
   $(public_command /usr/bin/git rev-parse HEAD) == "$head" &&
   $(public_command /usr/bin/git rev-parse 'HEAD^{tree}') == "$tree" &&
   -z $(public_command /usr/bin/git status --porcelain=v1 --untracked-files=all) ]] || {
  printf 'public dependency source or policy changed; retaining failed attempt\n' >&2; exit 1;
}
jq -se --arg head "$head" --arg tree "$tree" --arg component "$1" \
  --arg lock_sha256 "${lock_before%% *}" --arg policy_sha256 "${policy_before%% *}" \
  'if length == 4 and all(.[]; .exit_status == 0) then
    {schema_version:"jeryu.monorepo-candidate.dependency-sources/v1",component:$component,
     git:{head:$head,tree:$tree,clean:true},lock_sha256:$lock_sha256,policy_sha256:$policy_sha256,
     commands:.,conclusion:"success",installed_authority:false,public_origin_build:false}
   else error("dependency command admission incomplete") end' \
  "$attempt/commands.jsonl" > "$attempt/evidence.json"
printf 'public dependency sources passed; candidate evidence: %s/evidence.json\n' "$attempt"
