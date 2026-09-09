#!/usr/bin/env bash
# Retained command-line cache defenses and persisted Codegraph queries.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
umask 077
scratch=$(mktemp -d -t jeryu-product-proof.XXXXXXXX)
scratch_identity=$(stat -c '%d:%i' -- "$scratch")
cleanup() {
  local result=$?
  # Inspect links before deletion. Unlinking a link inside this owned scratch
  # never follows its target; a replaced root or nested mount is retained.
  if [[ -L "$scratch" || ! -d "$scratch" || $(realpath -e -- "$scratch") != "$scratch" \
        || $(stat -c '%d:%i' -- "$scratch") != "$scratch_identity" ]] \
      || findmnt -rn -o TARGET | awk -v root="$scratch" '$0 == root || index($0, root "/") == 1 {found=1} END {exit !found}'; then
    printf 'retaining changed or mounted proof scratch: %s\n' "$scratch" >&2
    exit 1
  fi
  find "$scratch" -xdev -type l -print > "$scratch/symlinks-before-cleanup.txt"
  rm -rf --one-file-system --preserve-root=all -- "$scratch"
  exit "$result"
}
trap cleanup EXIT
mkdir -p target/ci/product
evidence="$root/target/ci/product"
commit=$(git rev-parse HEAD)
source_state=clean
if ! git diff --quiet || ! git diff --cached --quiet || [[ -n $(git ls-files --others --exclude-standard) ]]; then
  source_state=working-tree
fi
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
printf 'Cache poisoning (7 scenarios) and Codegraph CLI persistence passed: base=%s source=%s.\n' "$commit" "$source_state"
