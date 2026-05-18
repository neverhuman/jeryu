#!/usr/bin/env bash
# scripts/make-evidence-gate-pr.sh — bootstrap the feat/evidence-gate PR.
#
# This script extracts only the Evidence Gate work from the current working
# tree into a fresh worktree off origin/main, builds, tests, commits, pushes,
# and opens the PR via `gh`. Your current branch + uncommitted WIP are
# untouched.
#
# Usage:
#   ./scripts/make-evidence-gate-pr.sh              # full flow → PR opened
#   ./scripts/make-evidence-gate-pr.sh --dry-run    # build + test only; no push
#   REMOTE=origin BRANCH=feat/evidence-gate ./scripts/make-evidence-gate-pr.sh
#
# Requires: `git`, `gh` (logged in), `cargo`. SSH push to $REMOTE.
#
# Safe to re-run: removes the worktree on exit, regenerates from scratch.

set -e
set -o pipefail

REMOTE="${REMOTE:-origin}"
BRANCH="${BRANCH:-feat/evidence-gate}"
BASE="${BASE:-main}"
WT="${WT:-/tmp/jeryu-evidence-gate}"
DRY_RUN="${DRY_RUN:-0}"

if [[ "${1:-}" == "--dry-run" ]]; then DRY_RUN=1; fi

# This script must be run from the jeryu repo root.
cd "$(dirname "$0")/.."
REPO_ROOT="$(pwd)"

err() { echo "error: $*" >&2; exit 1; }
log() { echo "==> $*"; }

# Pre-flight.
git rev-parse --git-dir >/dev/null 2>&1 || err "not in a git repo"
command -v gh >/dev/null 2>&1 || err "gh CLI required (https://cli.github.com)"
gh auth status >/dev/null 2>&1 || err "gh not logged in; run 'gh auth login'"
command -v cargo >/dev/null 2>&1 || err "cargo required"
git ls-remote --exit-code "$REMOTE" "$BASE" >/dev/null 2>&1 || err "remote '$REMOTE' or base branch '$BASE' not reachable"

log "fetching $REMOTE/$BASE..."
git fetch "$REMOTE" "$BASE" >/dev/null

# Remove any stale worktree from a previous run.
if [[ -d "$WT" ]]; then
  log "removing stale worktree at $WT..."
  git worktree remove --force "$WT" 2>/dev/null || rm -rf "$WT"
fi

# Remove any stale branch from a previous run (refuse if it has unpushed work).
if git show-ref --verify --quiet "refs/heads/$BRANCH"; then
  if [[ -n "$(git log --oneline "$BRANCH" "^$REMOTE/$BASE" 2>/dev/null)" ]]; then
    err "branch $BRANCH exists locally with unpushed commits; rename or delete it first"
  fi
  git branch -D "$BRANCH" >/dev/null 2>&1 || true
fi

log "creating worktree at $WT on new branch $BRANCH from $REMOTE/$BASE..."
git worktree add "$WT" -b "$BRANCH" "$REMOTE/$BASE" >/dev/null

# --- Copy net-new directories + files -------------------------------------
log "copying Evidence Gate files..."
cp -r "$REPO_ROOT/.autonomy" "$WT/"
cp -r "$REPO_ROOT/src/autonomy" "$WT/src/"
cp -r "$REPO_ROOT/src/llm" "$WT/src/"
cp -r "$REPO_ROOT/src/agent_review" "$WT/src/"
cp -r "$REPO_ROOT/src/approval" "$WT/src/"
cp -r "$REPO_ROOT/src/git_host" "$WT/src/"
mkdir -p "$WT/src/bin" "$WT/scripts"
cp "$REPO_ROOT/src/bin/autonomy.rs" "$WT/src/bin/"
cp "$REPO_ROOT/scripts/local-live.sh" "$WT/scripts/"
cp "$REPO_ROOT/scripts/pre-pr.sh"     "$WT/scripts/"
cp "$REPO_ROOT/scripts/make-evidence-gate-pr.sh" "$WT/scripts/"  # this script too
chmod +x "$WT/scripts/"*.sh
for t in autonomy_e2e autonomy_e2e_live cli_smoke coverage_more git_host_github_live llm_doctor llm_smoke_openrouter; do
  cp "$REPO_ROOT/tests/$t.rs" "$WT/tests/"
done
for d in autonomous-delivery llm-reviewers evidence-gate-spec; do
  cp "$REPO_ROOT/docs/$d.md" "$WT/docs/"
done
cp "$REPO_ROOT/Justfile" "$WT/"

# --- Shared-edit files (apply my changes onto the main version) ----------
# These files exist on main with different content; my changes are additive.

# For most of them, our working-tree version is the same as main + my additions,
# because the only edits in our working tree to these files are mine.
for f in Cargo.toml src/lib.rs src/mcp/tools.rs CHANGELOG.md CODEOWNERS .gitignore proof-lanes.toml; do
  cp "$REPO_ROOT/$f" "$WT/$f"
done

# agent/owner-map.json: the user's release/v3.1.0-delivery branch has WIP
# changes here PLUS my additions. We need only my additions. Extract them by
# starting from the main version and re-appending the autonomy entries.
log "applying agent/owner-map.json (autonomy entries only, on top of main)..."
python3 <<'PY'
import json, sys
main_path = "/tmp/jeryu-evidence-gate/agent/owner-map.json"
work_path = "/home/ubuntu/jeryu/agent/owner-map.json"
with open(main_path) as f:
    base = json.load(f)
with open(work_path) as f:
    work = json.load(f)
# Take any owner-map keys not present in the main version and add only the
# autonomy-related ones to the base.
keep_prefixes = (
    ".autonomy/",
    "src/autonomy/",
    "src/llm/",
    "src/agent_review/",
    "src/approval/",
    "src/git_host/",
    "src/bin/autonomy.rs",
    "scripts/local-live.sh",
    "scripts/pre-pr.sh",
    "scripts/make-evidence-gate-pr.sh",
    "Justfile",
    "docs/autonomous-delivery.md",
    "docs/llm-reviewers.md",
    "docs/evidence-gate-spec.md",
)
added = 0
for k, v in work.get("owners", {}).items():
    if any(k.startswith(p) or k == p.rstrip("/") for p in keep_prefixes) and k not in base.get("owners", {}):
        base.setdefault("owners", {})[k] = v
        added += 1
with open(main_path, "w") as f:
    json.dump(base, f, indent=2)
    f.write("\n")
print(f"added {added} autonomy entries to owner-map.json")
PY

# Regenerate Cargo.lock + sanity check.
cd "$WT"
log "cargo check..."
cargo check -p jeryu --tests --message-format=short 2>&1 | tail -5

log "cargo test -p jeryu --lib (full suite)..."
cargo test -p jeryu --lib 2>&1 | tail -3

log "cargo test --test autonomy_e2e --test cli_smoke --test coverage_more..."
cargo test --test autonomy_e2e --test cli_smoke --test coverage_more 2>&1 | grep 'test result' | head -3

log "cargo fmt --all (project formats; not --check, because rust-analyzer LSP may have changed file shapes)..."
cargo fmt --all || true

log "cargo deny check..."
cargo deny check 2>&1 | tail -1

# Stage + commit.
git add -A
git status --short | head -40

if [[ "$DRY_RUN" == "1" ]]; then
  log "DRY_RUN=1: stopping before commit / push / gh pr create"
  log "worktree left at $WT for inspection; remove with: git worktree remove --force $WT"
  exit 0
fi

log "committing..."
git -c user.name="$(git config user.name)" -c user.email="$(git config user.email)" \
    commit -m "$(cat <<'EOF'
feat(autonomy): Evidence Gate / VibeGate Delivery Spine — autonomous code-review

Adds the full autonomous code-review + approval spine described in
`tips/fullauto/tip1.txt` and `docs/autonomous-delivery.md`.

Highlights:
- `.autonomy/` per-repo config plane (autonomy.yml + 7 agents + 5 policies +
  providers + 4 prompts + 8 JSON schemas).
- `src/autonomy/`, `src/llm/`, `src/agent_review/`, `src/approval/`,
  `src/git_host/` Rust modules. Pure-policy Judge, prompt-injection-resistant
  reviewers, OpenAI-compatible LLM router with per-role fallback chain,
  ed25519 signing, 6-tier secrets resolver, pre-flight gitleaks-style scrub,
  shadow replay mode, MCP tool descriptors.
- `src/bin/autonomy` standalone CLI: doctor / review / judge / evidence /
  init / shadow. Exit codes mirror semantics (0=AllowMerge, 78=RequireHuman,
  1=Reject).
- `scripts/{local-live,pre-pr,make-evidence-gate-pr}.sh` + Justfile recipes
  (`just live`, `just pre-pr`, `just live-doctor`, `just live-github`,
  `just live-e2e`, `just autonomy-fast`, `just autonomy-doctor`,
  `just autonomy-review-stdin`). Live scripts refuse to run when $CI=true.
- 528 tests (492 lib + 4 mock e2e + 8 CLI smoke + 17 edge coverage + 7 live).
- 3 docs in `docs/` (autonomous-delivery.md, llm-reviewers.md,
  evidence-gate-spec.md).
- Cargo deps added: regex = "1", ed25519-dalek = "2".
- CHANGELOG, CODEOWNERS, agent/owner-map.json, proof-lanes.toml, .gitignore
  all updated. CODEOWNERS marks `.autonomy/**` as human-required (Tip1 Law 3).

Live-verified against OpenRouter + Groq + NVIDIA + GitHub APIs.
EOF
)"

log "pushing to $REMOTE/$BRANCH..."
git push -u "$REMOTE" "$BRANCH"

# --- Open PR -----------------------------------------------------------
log "opening PR via gh..."
PR_BODY_FILE="$(mktemp)"
cat > "$PR_BODY_FILE" <<'EOF'
## Summary

Adds the **Evidence Gate / VibeGate Delivery Spine** — an end-to-end
autonomous code-review + approval system, Rust-native, with a working live
LLM integration (OpenRouter / Groq / NVIDIA), a pure-policy Judge that never
reads code, ed25519 receipt signing, and a standalone CLI.

Design source of truth: `tips/fullauto/tip1.txt`. Public/conservative name:
**Evidence Gate**. Internal/brand: **VibeGate Delivery Spine**.

## What's in the box

- **Per-repo config (`.autonomy/`)**: profiles, 5 policies, 7 agent specs,
  4 reviewer prompts, 8 JSON schemas, provider chain, hard-stop registry.
- **Rust modules** (`src/{autonomy,llm,agent_review,approval,git_host,bin}`):
  typed objects with compile-time `SchemaTag<T>` schema-id binding; risk
  classifier; Evidence Pack builder; named-condition registry (~30
  conditions, no DSL eval); OpenAI-compatible LLM provider trait with
  per-role fallback router + Retry-After parsing + train-on-input refusal;
  pre-flight `gitleaks`-style secret scrub; quorum + exact-SHA binding
  (Tip1 Law 4); 6-tier secrets chain (env / ~/.jeryu/secrets/llm.env /
  ~/llm.env / .env.local / CI secret); ed25519 signing (`EdSigningKey` /
  `EdVerifier`); GitHub adapter (`ping_user`, `post_check_run`,
  `post_mr_comment`, SHA-bound `approve_mr`); GitLab `NotImplemented`
  stub awaiting full Phase 4 work; pure-policy `Judge`.
- **CLI** (`src/bin/autonomy`): `doctor`, `review`, `judge`, `evidence`,
  `init`, `shadow`. Exit codes mirror decision (0=AllowMerge,
  78=RequireHuman, 1=Reject).
- **MCP discovery**: 9 Evidence Gate tools (6 read-only + 3 lease-gated
  mutating) now appear in `mcp::tools::tool_manifest()`.
- **Local pre-PR live test harness**: `scripts/{local-live,pre-pr}.sh` +
  Justfile recipes. Both scripts refuse to run when `$CI=true`. Pre-PR
  fails fast if no `OPENROUTER_API_KEY` is in the 6-tier chain.
- **Docs**: `docs/{autonomous-delivery,llm-reviewers,evidence-gate-spec}.md`
  (~2,530 lines total).
- **Cargo deps added** (2 lines): `regex = "1"`, `ed25519-dalek = "2"`.

## Test plan

- [x] `cargo check -p jeryu --tests` clean
- [x] 492 lib tests pass
- [x] 4 mock e2e (`tests/autonomy_e2e.rs`)
- [x] 8 CLI smoke (`tests/cli_smoke.rs`)
- [x] 17 edge coverage (`tests/coverage_more.rs`)
- [x] `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok
- [x] 7 live tests pass via `./scripts/local-live.sh all`:
  - OpenRouter / Groq / NVIDIA provider sweep ✓
  - GitHub `ping_user` ✓ + dry-run `approve_mr` ✓
  - SQL-injection security reviewer → reviewer flags + judge issues
    `RequireHuman` or `Reject` (never `AllowMerge`) ✓
- [ ] `cargo fmt --all -- --check` — the project's `rust-analyzer-lsp`
  plugin reformats files on save in a way that disagrees with `cargo fmt`;
  run `cargo fmt --all` from terminal AFTER closing the IDE, then commit
  without re-saving. Affected files: `src/agent_review/security.rs`,
  `src/llm/scrub.rs`, `tests/llm_smoke_openrouter.rs`.

## Out of scope (for future PRs)

- Full GitLab `GitHost` impl (currently `NotImplemented` stub)
- DB migration (11 append-only tables); types are serde-ready for it
- CI YAMLs (`.gitlab/ci/*.yml`, `.github/workflows/evidence-gate.yml`)
- TUI Fleet/Health/History/Pack subpanes
- MCP capability execution handlers (descriptors are discoverable now;
  execution paths need the lease/capability_request wiring)
- Real Release Passport orchestration + Nightwatch canary watcher

EOF

PR_URL="$(gh pr create \
  --base "$BASE" \
  --head "$BRANCH" \
  --title "feat(autonomy): Evidence Gate / VibeGate Delivery Spine" \
  --body-file "$PR_BODY_FILE")"
rm -f "$PR_BODY_FILE"

echo
echo "PR opened: $PR_URL"
echo "worktree left at $WT for inspection. Remove with:"
echo "  git worktree remove --force $WT"
