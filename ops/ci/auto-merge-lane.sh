#!/usr/bin/env bash
# ops/ci/auto-merge-lane.sh — auto-merge gate logic for .github/workflows/auto-merge.yml.
#
# Decides whether a PR is eligible for GitHub auto-merge (squash). The workflow
# YAML calls into this script so the gate logic lives in one canonical place
# and so local rehearsal is possible:
#
#   gh pr view <N> --json number,draft,labels,headRefOid > /tmp/pr.json
#   bash ops/ci/auto-merge-lane.sh classify /tmp/pr.json
#
# Stages:
#   classify <pr.json>   — read PR metadata, emit "skip|enable" + reason to stdout.
#
# This script is intentionally pure (read-only) — the actual auto-merge
# mutation is invoked from the workflow via the GitHub GraphQL API after this
# script returns "enable". That keeps the GraphQL token scope narrow.

set -euo pipefail

usage() {
    printf 'usage: %s classify <pr-metadata-json>\n' "$0" >&2
    exit 2
}

case "${1:-}" in
    classify)
        shift
        ;;
    *)
        usage
        ;;
esac

PR_JSON="${1:-}"
[ -n "$PR_JSON" ] && [ -r "$PR_JSON" ] || { echo "missing or unreadable PR metadata: $PR_JSON" >&2; exit 2; }

# Read fields via python (the GitHub-hosted runner always has python3).
classify() {
    python3 - "$PR_JSON" <<'PYEOF'
import json, sys
data = json.load(open(sys.argv[1]))

# Single PR dict (from gh pr view --json) or list with one entry — accept both.
pr = data[0] if isinstance(data, list) else data

# Draft PRs always skipped.
if pr.get('draft') or pr.get('isDraft'):
    print("skip\tdraft")
    sys.exit(0)

# Label opt-out.
labels = pr.get('labels') or []
label_names = {l.get('name') if isinstance(l, dict) else l for l in labels}
if 'no-auto-merge' in label_names:
    print("skip\tlabel:no-auto-merge")
    sys.exit(0)

# Default: eligible.
print("enable\tready")
PYEOF
}

classify
