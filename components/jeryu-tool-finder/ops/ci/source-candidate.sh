#!/usr/bin/env bash
# Explicit candidate source scope. Legacy split authority remains in source-authority.sh.

jeryu_candidate_root_git() (
  local repo_root=$1
  shift
  jeryu_governed_git "$@"
)

jeryu_candidate_source_root() {
  local root
  [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 && ${JAIN_RELEASE_CI:-0} != 1 &&
     ${JERYU_MONOREPO_EXPECTED_HEAD:-} =~ ^[0-9a-f]{40}$ ]] ||
    { jeryu_source_fail 'candidate source requires an exact unendorsed commit'; return 1; }
  [[ ${repo_root:-} == /* && -d $repo_root && ! -L $repo_root &&
     $(realpath -e -- "$repo_root") == "$repo_root" ]] ||
    { jeryu_source_fail 'candidate component root is not physical'; return 1; }
  root=$(cd -- "$repo_root/../.." && pwd -P) || return 1
  [[ $repo_root == "$root/components/jeryu-tool-finder" &&
     -d $root/.git && ! -L $root/.git &&
     ! -e $repo_root/.git && ! -L $repo_root/.git ]] ||
    { jeryu_source_fail 'candidate requires the exact Finder component of one monorepo'; return 1; }
  [[ $(jeryu_candidate_root_git "$root" rev-parse --show-toplevel) == "$root" &&
     $(jeryu_candidate_root_git "$root" rev-parse --verify 'HEAD^{commit}') == "$JERYU_MONOREPO_EXPECTED_HEAD" ]] ||
    { jeryu_source_fail 'candidate monorepo root or commit differs'; return 1; }
  printf '%s\n' "$root"
}

jeryu_candidate_shared_source_checks() {
  local root=$1
  jeryu_candidate_root_git "$root" ls-files -v -z |
    while IFS= read -r -d '' record; do
      [[ $record == 'H '* ]] ||
        { jeryu_source_fail 'shared source has hidden or nonordinary index state'; return 1; }
    done || return 1
  jeryu_candidate_root_git "$root" diff-index --cached --quiet HEAD -- ||
    { jeryu_source_fail 'shared index differs from literal HEAD'; return 1; }
  jeryu_candidate_root_git "$root" diff-files --quiet -- ||
    { jeryu_source_fail 'shared tracked bytes differ from the index'; return 1; }
  [[ -z $(jeryu_candidate_root_git "$root" status --porcelain=v1 --untracked-files=all) ]] ||
    { jeryu_source_fail 'shared source checkout is not clean'; return 1; }
  # Finder retains its stricter component list in source-authority.sh. Other
  # shared paths use only the derived patterns already present in root .gitignore.
  jeryu_candidate_root_git "$root" ls-files --others --ignored --exclude-standard -z |
    while IFS= read -r -d '' ignored; do
      case "$ignored" in
        components/jeryu-tool-finder/*) continue ;;
      esac
      case "/$ignored" in
        */target/*|*/node_modules/*|*/dist/*|*/playwright-report/*|*/test-results/*|\
        */storybook-static/*|*/.jankurai/*|*.tsbuildinfo) ;;
        *) jeryu_source_fail "ignored shared input outside a derived zone: $ignored"; return 1 ;;
      esac
    done || return 1
}

jeryu_candidate_verify_whole_source() {
  local root=$1 verifier
  verifier="$root/components/jeryu-tool/ops/verify-public-candidate.sh"
  [[ -f $verifier && ! -L $verifier &&
     $(realpath -e -- "$verifier") == "$verifier" ]] ||
    { jeryu_source_fail 'candidate source lacks its fixed monorepo verifier'; return 1; }
  # No caller verifier, receipt fixture, or protected-baseline override is accepted.
  # shellcheck source=/dev/null
  source "$verifier"
  require_public_candidate_jankurai
}

jeryu_candidate_shared_inputs_sha256() {
  local root=$1
  # No path filters: shared manifests, root tooling and every component participate.
  jeryu_candidate_root_git "$root" ls-files -z |
    while IFS= read -r -d '' path; do
      local physical mode digest before after
      [[ $path != *$'\n'* && $path != *$'\r'* ]] ||
        { jeryu_source_fail 'shared tracked path contains a line break'; return 1; }
      physical="$root/$path"
      [[ -f $physical && ! -L $physical &&
         $(realpath -e -- "$physical") == "$physical" &&
         $(stat -c %h -- "$physical") == 1 ]] ||
        { jeryu_source_fail "shared tracked input is not physical and single-link: $path"; return 1; }
      before=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$physical") || return 1
      mode=$(stat -c %a -- "$physical") || return 1
      digest=$(sha256sum -- "$physical" | awk '{print $1}') || return 1
      after=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$physical") || return 1
      [[ $before == "$after" && ! -L $physical &&
         $(realpath -e -- "$physical") == "$physical" ]] ||
        { jeryu_source_fail "shared tracked input moved while hashing: $path"; return 1; }
      printf '%s\0%s\0%s\0' "$path" "$mode" "$digest"
    done | sha256sum | awk '{print $1}'
}

jeryu_candidate_source_scope() {
  local head=$1 tree=$2 component_inputs=$3 root component_tree shared_inputs
  root=$(jeryu_candidate_source_root) || return 1
  [[ $head == "$JERYU_MONOREPO_EXPECTED_HEAD" &&
     $(jeryu_candidate_root_git "$root" rev-parse 'HEAD^{tree}') == "$tree" ]] ||
    { jeryu_source_fail 'candidate scope differs from the bound source'; return 1; }
  component_tree=$(jeryu_candidate_root_git "$root" rev-parse "$head:components/jeryu-tool-finder") || return 1
  [[ $component_tree =~ ^[0-9a-f]{40}$ &&
     $(jeryu_candidate_root_git "$root" cat-file -t "$component_tree") == tree ]] ||
    { jeryu_source_fail 'candidate Finder subtree is not a tree'; return 1; }
  shared_inputs=$(jeryu_candidate_shared_inputs_sha256 "$root") || return 1
  [[ $component_inputs =~ ^[0-9a-f]{64}$ && $shared_inputs =~ ^[0-9a-f]{64}$ &&
     $(jeryu_candidate_root_git "$root" rev-parse HEAD) == "$head" &&
     $(jeryu_candidate_root_git "$root" rev-parse 'HEAD^{tree}') == "$tree" ]] ||
    { jeryu_source_fail 'candidate scope moved while hashing shared inputs'; return 1; }
  jeryu_candidate_shared_source_checks "$root" || return 1
  jq -cnS --arg head "$head" --arg tree "$tree" --arg shared "$shared_inputs" \
    --arg component_tree "$component_tree" --arg component_inputs "$component_inputs" '
    {schema:"jeryu.monorepo-candidate.source/v1",
     monorepo:{repository:"https://github.com/neverhuman/jeryu.git",commit:$head,
       tree:$tree,tracked_inputs_sha256:$shared},
     component:{path:"components/jeryu-tool-finder",tree:$component_tree,
       tracked_inputs_sha256:$component_inputs},
     governance:{protected_main:false,handover:"pending"}}'
}

jeryu_candidate_score_auditor() {
  [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 &&
     ${JERYU_CANDIDATE_JANKURAI_DESCRIPTOR:-} == /proc/[0-9]*/fd/[0-9]* ]] ||
    { jeryu_source_fail 'candidate score lacks its current verified auditor descriptor'; return 1; }
  jeryu_with_scrubbed_git "$JERYU_CANDIDATE_JANKURAI_DESCRIPTOR" "$@"
}
