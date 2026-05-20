#!/usr/bin/env bash
# scripts/publish-pr.sh — gate PR publication with local CI parity.
#
# Usage:
#   bash scripts/publish-pr.sh -- gh pr create --fill
#   bash scripts/publish-pr.sh --remote origin --branch feat/foo -- gh pr create --base main --head feat/foo --fill
#
# The command after `--` is executed after `just ci-parity` and `git push`.

set -euo pipefail

cd "$(dirname "$0")/.."

REMOTE="${REMOTE:-origin}"
BRANCH="${BRANCH:-}"
BASE="${BASE:-main}"
RUN_CI="${RUN_CI:-1}"

command -v git >/dev/null 2>&1 || {
    echo "error: git is required" >&2
    exit 1
}
command -v just >/dev/null 2>&1 || {
    echo "error: just is required for the ci-parity gate" >&2
    exit 1
}
command -v gh >/dev/null 2>&1 || {
    echo "error: gh is required for PR publication" >&2
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --remote)
            REMOTE="$2"
            shift 2
            ;;
        --branch)
            BRANCH="$2"
            shift 2
            ;;
        --base)
            BASE="$2"
            shift 2
            ;;
        --skip-ci)
            RUN_CI=0
            shift
            ;;
        --)
            shift
            break
            ;;
        *)
            break
            ;;
    esac
done

if [[ -z "$BRANCH" ]]; then
    BRANCH="$(git branch --show-current)"
fi

if [[ -z "$BRANCH" ]]; then
    echo "error: branch is required" >&2
    exit 2
fi

if [[ "$RUN_CI" == "1" ]]; then
    just ci-parity >&2
fi

git push -u "$REMOTE" "$BRANCH" >&2

if [[ $# -eq 0 ]]; then
    exit 0
fi

"$@"
