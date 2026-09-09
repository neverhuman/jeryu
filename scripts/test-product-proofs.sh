#!/usr/bin/env bash
# Retained command-line cache defenses and persisted Codegraph queries.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
umask 077
commit=$(git rev-parse HEAD)
tree=$(git rev-parse 'HEAD^{tree}')
source_status=$(git status --porcelain=v1 --untracked-files=all) || {
  printf 'could not inspect product proof source state\n' >&2; exit 1;
}
[[ -z $source_status ]] || {
  printf 'product proof requires a clean source tree\n' >&2; exit 1;
}
for directory in "$root/target" "$root/target/ci" "$root/target/ci/product"; do
  if [[ ! -e $directory && ! -L $directory ]]; then mkdir -m 0700 -- "$directory"; fi
  [[ -d $directory && ! -L $directory && -O $directory &&
     $(realpath -e -- "$directory") == "$directory" ]] || {
    printf 'product evidence directory is not physical and owned\n' >&2; exit 1;
  }
done
evidence=$(mktemp -d "$root/target/ci/product/attempt.XXXXXXXX")
evidence_identity=$(stat -c '%d:%i:%u:%g:%a' -- "$evidence")
# shellcheck source=tests/scratch.sh
source "$root/tests/scratch.sh"
scratch=$(mktemp -d -t jeryu-product-proof.XXXXXXXX)
jeryu_record_test_scratch "$scratch"
cleanup() {
  local result=$?
  jeryu_remove_test_scratch || result=1
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
jq -n --arg commit "$commit" --arg tree "$tree" \
  '{source_commit:$commit,source_tree:$tree,source_state:"clean"}' >"$evidence/source.json"
cargo run --locked -p jeryu-cache --bin jeryu-cache -- self-test "$scratch/cache" | tee "$evidence/cache.log"
for assertion in \
  'ok: fork PR cannot write trusted cache' \
  'ok: cross-project read denied by default' \
  'ok: release ignores mutable cache' \
  'ok: cache service outage safe-miss' \
  'ok: false-hit detector' \
  'phase6 adversarial suite: ok (7 scenarios)'; do
  rg --fixed-strings --quiet "$assertion" "$evidence/cache.log"
done
[[ $(rg -c '^ok:' "$evidence/cache.log") == 7 ]]
! rg -q '^FAILED:' "$evidence/cache.log"

cargo run --locked -p jeryu-codegraph -- tool-build scan \
  --root "$root/components/jeryu-intelligence" --db "$scratch/codegraph.sqlite" \
  --repo-id local/jeryu --commit "$commit" --top 10 --json > "$evidence/codegraph-scan.json"
cargo run --locked -p jeryu-codegraph -- tool-build clusters \
  --db "$scratch/codegraph.sqlite" --repo-id local/jeryu --top 10 --json > "$evidence/codegraph-clusters.json"
jq -e --arg commit "$commit" '.repo_id == "local/jeryu" and .commit_sha == $commit and .scanned_files > 0 and (.clusters | length) > 0' \
  "$evidence/codegraph-scan.json" >/dev/null
jq -e -s '.[0].clusters | sort_by(.cluster_id)' "$evidence/codegraph-scan.json" > "$scratch/expected.json"
jq -e 'sort_by(.cluster_id)' "$evidence/codegraph-clusters.json" > "$scratch/actual.json"
cmp "$scratch/expected.json" "$scratch/actual.json"
source_status=$(git status --porcelain=v1 --untracked-files=all) || {
  printf 'could not recheck product proof source state\n' >&2; exit 1;
}
[[ $(git rev-parse HEAD) == "$commit" && $(git rev-parse 'HEAD^{tree}') == "$tree" &&
   -z $source_status ]] || {
  printf 'source changed during product proof\n' >&2; exit 1;
}
[[ -d $evidence && ! -L $evidence && $(realpath -e -- "$evidence") == "$evidence" &&
   $(stat -c '%d:%i:%u:%g:%a' -- "$evidence") == "$evidence_identity" ]] || {
  printf 'product evidence directory changed during execution\n' >&2; exit 1;
}
jeryu_remove_test_scratch
trap - EXIT
printf 'Cache poisoning (7 scenarios) and Codegraph CLI persistence passed: source=%s evidence=%s.\n' "$commit" "$evidence"
