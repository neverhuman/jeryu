#!/usr/bin/env bash
#
# backfill-repo-scores.sh - one-time jankurai score backfill for the forge registry.
#
# Audits each canonical jeryu/<name> repo's default branch with the PINNED jankurai
# and POSTs the result to the forge ingest endpoint
# (POST $API/api/v1/repos/jeryu/<name>/jankurai-scores), so every repo carries a
# score — not just the ones that have been through PR CI since ingest landed.
#
# Properties:
#   * Audits run against a `git clone file://<bare>` copy in a temp dir; the live
#     bare repos under $DATA_DIR are NEVER touched (file:// forces a real object
#     copy — never hardlinks into the live bare).
#   * Tool failure (e.g. non-Rust repos the auditor cannot score) is an EXPECTED
#     outcome: recorded as decision="tool-failed" and never aborts the sweep.
#   * Idempotent: a default-branch SHA that already has an ingested score is
#     skipped. Any non-200 / unparseable response from the GET means "no skip
#     data, proceed" — the endpoint may not exist yet.
#
# Usage:
#   ops/jankurai/backfill-repo-scores.sh [--dry-run]
#
# Env:
#   JERYU_API            forge API base                 (default http://127.0.0.1:8787)
#   JERYU_DATA_DIR       forge data dir                 (default ~/.local/share/jeryu)
#   JERYU_BACKFILL_JOBS  parallel audits, clamped to 3  (default 2)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "${ROOT}/ops/ci/common.sh"

API="${JERYU_API:-http://127.0.0.1:8787}"
DATA_DIR="${JERYU_DATA_DIR:-$HOME/.local/share/jeryu}"
JOBS="${JERYU_BACKFILL_JOBS:-2}"
# This host has been wedged by unbounded workers before — clamp hard, never trust a
# requested fan-out.
if [ "${JOBS}" -gt 3 ]; then
  JOBS=3
fi

DRY_RUN=0
FORCE=0
for arg in "$@"; do
  case "${arg}" in
    --dry-run) DRY_RUN=1 ;;
    # Re-audit and re-ingest even when the SHA already carries a score; the
    # ingest endpoint upserts per (branch, commit_sha), so this replaces
    # records (used to repair a sweep that audited bad checkouts).
    --force) FORCE=1 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: ${arg}" >&2; exit 2 ;;
  esac
done

# Verify the pinned auditor ONCE up front; every invocation below uses the explicit
# "${JERYU_JANKURAI_BIN}" path — never bare `jankurai`, which a stale 1.5.1 build
# earlier on PATH shadows on this host.
ensure_pinned_jankurai

# Per-repo stdout/stderr logs and outcome records live here; the dir survives the run
# so the operator can inspect failures.
LOG_DIR="$(mktemp -d -t jankurai-backfill-XXXXXX)"
OUT_DIR="${LOG_DIR}/outcomes"
mkdir -p "${OUT_DIR}"

# Enumerate the jeryu-owned registry repos (name + default branch). The legacy
# `local/` owner entries are not canonical and are skipped here.
listing_json="${LOG_DIR}/repos.json"
curl -fsS "${API}/api/v1/repos" -o "${listing_json}"
repos_tsv="${LOG_DIR}/repos.tsv"
python3 - "${listing_json}" > "${repos_tsv}" <<'PY'
import json
import sys

doc = json.load(open(sys.argv[1]))
for repo in doc["repositories"]:
    rid = repo["id"]
    if rid["owner"] != "jeryu":
        continue
    print(f'{rid["name"]}\t{repo["default_branch"]}')
PY
mapfile -t REPO_LINES < "${repos_tsv}"
echo "[backfill] ${#REPO_LINES[@]} jeryu-owned repos in the registry (jobs=${JOBS}, logs=${LOG_DIR})"

record() {
  local name="$1" outcome="$2"
  printf '%s\n' "${outcome}" > "${OUT_DIR}/${name}.outcome"
}

# already_scored <name> <sha>
#
# Idempotency probe. Returns 0 ONLY when the ingest endpoint answers with JSON that
# carries at least one score for this SHA. A non-200, an HTML SPA fallback, or any
# parse failure all return 1 ("no skip data, proceed") — the endpoint may not exist
# on this forge build yet.
already_scored() {
  local name="$1" sha="$2" body
  body="$(curl -fsS "${API}/api/v1/repos/jeryu%2F${name}/jankurai-scores?sha=${sha}" 2>/dev/null || true)"
  [ -n "${body}" ] || return 1
  printf '%s' "${body}" | python3 -c '
import json
import sys

try:
    doc = json.load(sys.stdin)
except ValueError:
    sys.exit(1)
scores = doc if isinstance(doc, list) else doc.get("scores") or []
sys.exit(0 if scores else 1)
'
}

# audit_one <name> <branch>
#
# Runs as a background job: resolve the branch SHA from the live bare repo, clone a
# throwaway copy, audit it with the pinned jankurai, and POST the outcome. Every
# fallible step is guarded so one repo can never abort the sweep; the temp clone is
# always removed via the job-local EXIT trap.
audit_one() {
  local name="$1" branch="$2"
  local bare="${DATA_DIR}/git/jeryu/${name}.git"
  local log="${LOG_DIR}/${name}.log"

  if [ ! -d "${bare}" ]; then
    echo "[backfill] jeryu/${name}: SKIP (no bare repo at ${bare})"
    record "${name}" "skipped: no bare repo"
    return 0
  fi

  local sha
  if ! sha="$(git --git-dir "${bare}" rev-parse "refs/heads/${branch}" 2>>"${log}")"; then
    echo "[backfill] jeryu/${name}: SKIP (refs/heads/${branch} unresolvable — empty repo?)"
    record "${name}" "skipped: refs/heads/${branch} unresolvable"
    return 0
  fi

  if [ "${FORCE}" -ne 1 ] && already_scored "${name}" "${sha}"; then
    echo "[backfill] jeryu/${name}: SKIP (score already ingested for ${sha})"
    record "${name}" "skipped: already scored @ ${sha}"
    return 0
  fi

  if [ "${DRY_RUN}" -eq 1 ]; then
    echo "WOULD AUDIT jeryu/${name} @ ${sha}"
    record "${name}" "would-audit @ ${sha}"
    return 0
  fi

  local tmp
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' EXIT
  # file:// forces a real object copy — never hardlink into the live bare repo.
  # --branch is load-bearing: several bares carry a HEAD that points at a
  # nonexistent ref, and a default clone then checks out NOTHING — the auditor
  # silently scores an empty tree (uniform bogus low scores).
  if ! git clone -q --branch "${branch}" "file://${bare}" "${tmp}/src" >>"${log}" 2>&1; then
    echo "[backfill] jeryu/${name}: CLONE FAILED (see ${log})" >&2
    record "${name}" "failed: clone failed (see ${log})"
    return 0
  fi
  if [ ! -e "${tmp}/src/.git/HEAD" ] || [ -z "$(ls -A "${tmp}/src" | grep -v '^\.git$')" ]; then
    echo "[backfill] jeryu/${name}: EMPTY CHECKOUT — refusing to audit nothing" >&2
    record "${name}" "failed: empty checkout for ${branch}"
    return 0
  fi

  # Same audit invocation + artifact path as ops/ci/jankurai.sh's full-mode audit
  # (no jeryu-specific --policy: each repo is audited under its own policy). A
  # nonzero exit does NOT mean the tool failed — a low score also exits nonzero —
  # so the discriminator is whether repo-score.json was produced and parses.
  local rc=0
  (
    cd "${tmp}/src"
    "${JERYU_JANKURAI_BIN}" audit . \
      --json .jankurai/repo-score.json \
      --md .jankurai/repo-score.md \
      --no-score-history
  ) >>"${log}" 2>&1 || rc=$?

  local payload outcome
  if payload="$(python3 - "${tmp}/src/.jankurai/repo-score.json" "${branch}" "${sha}" 2>>"${log}" <<'PY'
import json
import sys

report = json.load(open(sys.argv[1]))
decision = report.get("decision") or {}
print(json.dumps({
    "branch": sys.argv[2],
    "commit_sha": sys.argv[3],
    "score": report.get("score"),
    "hard_findings": decision.get("hard_findings"),
    "decision": "scored",
    "caps_applied": report.get("caps_applied") or [],
    "report": report,
}))
PY
  )"; then
    outcome="scored"
  else
    # EXPECTED for non-Rust repos: record the tool failure instead of a score.
    payload="$(python3 -c '
import json
import sys

print(json.dumps({
    "branch": sys.argv[1],
    "commit_sha": sys.argv[2],
    "score": None,
    "decision": "tool-failed",
    "tool_exit": int(sys.argv[3]),
}))
' "${branch}" "${sha}" "${rc}")"
    outcome="tool-failed (exit ${rc})"
  fi

  if curl -fsS -X POST "${API}/api/v1/repos/jeryu%2F${name}/jankurai-scores" \
    -H 'content-type: application/json' --data "${payload}" >>"${log}" 2>&1; then
    echo "[backfill] jeryu/${name}: ${outcome} @ ${sha}"
    record "${name}" "${outcome} @ ${sha}"
  else
    echo "[backfill] jeryu/${name}: POST FAILED (see ${log})" >&2
    record "${name}" "post-failed: ingest POST rejected (see ${log})"
  fi
}

# Bounded job pool — the ONLY parallelism in this script.
for line in "${REPO_LINES[@]}"; do
  name="${line%%$'\t'*}"
  branch="${line#*$'\t'}"
  while [ "$(jobs -rp | wc -l)" -ge "${JOBS}" ]; do
    wait -n
  done
  audit_one "${name}" "${branch}" &
done
wait

echo
echo "[backfill] summary:"
failures=0
for line in "${REPO_LINES[@]}"; do
  name="${line%%$'\t'*}"
  outcome="$(cat "${OUT_DIR}/${name}.outcome" 2>/dev/null || echo "no outcome recorded")"
  printf '  %-28s %s\n' "jeryu/${name}" "${outcome}"
  case "${outcome}" in
    failed*|post-failed*|"no outcome recorded") failures=$((failures + 1)) ;;
  esac
done
echo "[backfill] logs: ${LOG_DIR}"

# tool-failed entries are expected outcomes; only failed ingests fail the sweep.
if [ "${failures}" -gt 0 ]; then
  echo "[backfill] FAILED: ${failures} repo(s) could not be ingested" >&2
  exit 1
fi
echo "[backfill] done"
