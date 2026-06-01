#!/usr/bin/env bash
# Publish the managed README score block through the local API and verify the
# round-trip before a branch is pushed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"
source "${ROOT}/ops/ci/common.sh"

API_URL="${JERYU_README_PUBLISH_API_URL:-http://127.0.0.1:8787}"
REPO_ID="${JERYU_README_PUBLISH_REPO:-}"
README_PATH="${JERYU_README_PUBLISH_README:-README.md}"
SCORE_JSON="${JERYU_README_PUBLISH_SCORE_JSON:-target/jankurai/repo-score.json}"
SCORE_MD="${JERYU_README_PUBLISH_SCORE_MD:-target/jankurai/repo-score.md}"
RECEIPT_PATH="${JERYU_README_PUBLISH_RECEIPT:-target/jankurai/readme-publish-receipt.json}"
DRY_RUN=0
VERIFY=1
repo_name="$(basename "$(git rev-parse --show-toplevel)")"
repo_list=""
managed_block=""
canonical_readme=""
rendered_readme=""
verified_readme=""
request_json=""
response_json=""
get_json=""

cleanup() {
  rm -f \
    "${repo_list:-}" \
    "${managed_block:-}" \
    "${canonical_readme:-}" \
    "${rendered_readme:-}" \
    "${verified_readme:-}" \
    "${request_json:-}" \
    "${response_json:-}" \
    "${get_json:-}"
}
trap cleanup EXIT

for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --verify) VERIFY=1 ;;
    --api-url=*) API_URL="${arg#--api-url=}" ;;
    --repo=*) REPO_ID="${arg#--repo=}" ;;
    --readme=*) README_PATH="${arg#--readme=}" ;;
    --score-json=*) SCORE_JSON="${arg#--score-json=}" ;;
    --score-md=*) SCORE_MD="${arg#--score-md=}" ;;
    --receipt=*) RECEIPT_PATH="${arg#--receipt=}" ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

if [ ! -f "$SCORE_JSON" ] || [ ! -f "$SCORE_MD" ]; then
  echo "missing fresh Jankurai artifacts: $SCORE_JSON and/or $SCORE_MD" >&2
  echo "run bash ops/ci/proof-evidence.sh before publishing the managed README block" >&2
  exit 1
fi

if [ -z "$REPO_ID" ]; then
  repo_list="$(mktemp)"
  if curl --fail --silent --show-error \
    -H 'Accept: application/json' \
    "$API_URL/api/v1/repos" > "$repo_list"; then
    resolved_repo="$(jq -r --arg name "$repo_name" '.repositories[] | select(.id.name == $name) | .id.owner + "/" + .id.name' "$repo_list" | head -n1)"
    if [ -n "$resolved_repo" ]; then
      REPO_ID="$resolved_repo"
    fi
  fi
  if [ -z "$REPO_ID" ]; then
    remote_url="$(git remote get-url origin 2>/dev/null || true)"
    if [ -n "$remote_url" ]; then
      REPO_ID="$(printf '%s\n' "$remote_url" | sed -E 's#.*[:/]([^/]+/[^/]+)(\.git)?$#\1#; s#\.git$##')"
      if [ -n "$REPO_ID" ] && [ "$REPO_ID" != "$remote_url" ]; then
        :
      else
        REPO_ID=""
      fi
    fi
  fi
  if [ -z "$REPO_ID" ]; then
    REPO_ID="local/$repo_name"
  fi
fi

mkdir -p "$(dirname "$RECEIPT_PATH")"

score="$(jq -r '.score' "$SCORE_JSON")"
raw_score="$(jq -r '.raw_score' "$SCORE_JSON")"
hard_findings="$(jq -r '.decision.hard_findings' "$SCORE_JSON")"
soft_findings="$(jq -r '.decision.soft_findings' "$SCORE_JSON")"
caps="$(jq -r 'if .decision.ratchet.new_caps | length == 0 then "none" else .decision.ratchet.new_caps | join(", ") end' "$SCORE_JSON")"
report_fingerprint="$(jq -r '.report_fingerprint' "$SCORE_JSON")"

managed_block="$(mktemp)"
canonical_readme="$(mktemp)"
rendered_readme="$(mktemp)"
verified_readme="$(mktemp)"
request_json="$(mktemp)"
response_json="$(mktemp)"
get_json="$(mktemp)"

cat >"$managed_block" <<EOF
- Final score: \`$score\`
- Raw score: \`$raw_score\`
- Hard findings: \`$hard_findings\`
- Soft findings: \`$soft_findings\`
- Caps applied: \`$caps\`
- Report fingerprint: \`$report_fingerprint\`
- Source artifacts: \`$SCORE_JSON\`, \`$SCORE_MD\`
- Publish receipt: \`$RECEIPT_PATH\`
EOF

render_readme() {
  local source="$1"
  local block="$2"
  local target="$3"
  awk -v block_file="$block" '
    BEGIN {
      start = "<!-- jeryu:managed-score:start -->";
      end = "<!-- jeryu:managed-score:end -->";
      skip = 0;
    }
    $0 == start {
      print $0;
      while ((getline line < block_file) > 0) print line;
      close(block_file);
      skip = 1;
      next;
    }
    skip && $0 == end {
      print $0;
      skip = 0;
      next;
    }
    !skip {
      print $0;
    }
  ' "$source" > "$target"
}

render_readme "$README_PATH" "$managed_block" "$canonical_readme"

canonical_sha="$(sha256sum "$canonical_readme" | awk '{print $1}')"
block_sha="$(sha256sum "$managed_block" | awk '{print $1}')"

if [ "$DRY_RUN" = "1" ]; then
  jq -n \
    --arg repo "$REPO_ID" \
    --arg api_url "$API_URL" \
    --arg readme_path "$README_PATH" \
    --arg score_json "$SCORE_JSON" \
    --arg score_md "$SCORE_MD" \
    --arg receipt_path "$RECEIPT_PATH" \
    --arg canonical_sha "$canonical_sha" \
    --arg block_sha "$block_sha" \
    --arg score "$score" \
    --arg raw_score "$raw_score" \
    --arg hard_findings "$hard_findings" \
    --arg soft_findings "$soft_findings" \
    --arg caps "$caps" \
    --arg report_fingerprint "$report_fingerprint" \
    --argjson dry_run true \
    --argjson verified true \
    '{
      repo: $repo,
      api_url: $api_url,
      readme_path: $readme_path,
      score_json: $score_json,
      score_md: $score_md,
      receipt_path: $receipt_path,
      canonical_sha256: $canonical_sha,
      managed_block_sha256: $block_sha,
      score: ($score | tonumber),
      raw_score: ($raw_score | tonumber),
      hard_findings: ($hard_findings | tonumber),
      soft_findings: ($soft_findings | tonumber),
      caps: $caps,
      report_fingerprint: $report_fingerprint,
      dry_run: $dry_run,
      verified: $verified
    }' > "$RECEIPT_PATH"
  echo "dry-run complete: managed README block rendered for ${REPO_ID}"
  exit 0
fi

repo_path="$(jq -nr --arg v "$REPO_ID" '$v|@uri')"

jq -n --rawfile markdown "$canonical_readme" '{markdown: $markdown}' > "$request_json"

curl --fail --silent --show-error \
  -H 'Content-Type: application/json' \
  -X PUT \
  --data @"$request_json" \
  "$API_URL/api/v1/repos/$repo_path/readme" > "$response_json"

jq -j '.markdown' "$response_json" > "$rendered_readme"
if ! cmp -s "$canonical_readme" "$rendered_readme"; then
  echo "API returned a different README markdown payload than requested" >&2
  exit 1
fi

if [ "$VERIFY" = "1" ]; then
  curl --fail --silent --show-error \
    -H 'Accept: application/json' \
    "$API_URL/api/v1/repos/$repo_path/readme" > "$get_json"
  jq -j '.markdown' "$get_json" > "$verified_readme"
  if ! cmp -s "$canonical_readme" "$verified_readme"; then
    echo "README round-trip verification failed" >&2
    exit 1
  fi
fi

jq -j '.markdown' "$response_json" > "$README_PATH"

published_sha="$(sha256sum "$README_PATH" | awk '{print $1}')"
returned_sha="$(sha256sum "$rendered_readme" | awk '{print $1}')"
get_sha="$(sha256sum "$get_json" | awk '{print $1}')"

jq -n \
  --arg repo "$REPO_ID" \
  --arg api_url "$API_URL" \
  --arg readme_path "$README_PATH" \
  --arg score_json "$SCORE_JSON" \
  --arg score_md "$SCORE_MD" \
  --arg receipt_path "$RECEIPT_PATH" \
  --arg canonical_sha "$canonical_sha" \
  --arg block_sha "$block_sha" \
  --arg published_sha "$published_sha" \
  --arg returned_sha "$returned_sha" \
  --arg get_sha "$get_sha" \
  --arg score "$score" \
  --arg raw_score "$raw_score" \
  --arg hard_findings "$hard_findings" \
  --arg soft_findings "$soft_findings" \
  --arg caps "$caps" \
  --arg report_fingerprint "$report_fingerprint" \
  --argjson dry_run false \
  --argjson verified true \
  '{
    repo: $repo,
    api_url: $api_url,
    readme_path: $readme_path,
    score_json: $score_json,
    score_md: $score_md,
    receipt_path: $receipt_path,
    canonical_sha256: $canonical_sha,
    managed_block_sha256: $block_sha,
    published_readme_sha256: $published_sha,
    returned_markdown_sha256: $returned_sha,
    get_response_sha256: $get_sha,
    score: ($score | tonumber),
    raw_score: ($raw_score | tonumber),
    hard_findings: ($hard_findings | tonumber),
    soft_findings: ($soft_findings | tonumber),
    caps: $caps,
    report_fingerprint: $report_fingerprint,
    dry_run: $dry_run,
    verified: $verified
  }' > "$RECEIPT_PATH"

echo "published README score block for ${REPO_ID}"
