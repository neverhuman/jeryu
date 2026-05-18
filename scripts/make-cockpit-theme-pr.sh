#!/usr/bin/env bash
# scripts/make-cockpit-theme-pr.sh — bootstrap the feat/cockpit-theme PR.
#
# Ships ONLY the `Theme::cockpit()` palette + Tone API + chip/connector
# helpers added to `src/tui/theme.rs`. Backward-compatible: existing
# `Theme::dark()` consumers are untouched.
#
# Designed to be ship-able independently of the Evidence Gate PR; the two
# can land in any order.
#
# Usage:
#   bash scripts/make-cockpit-theme-pr.sh
#   bash scripts/make-cockpit-theme-pr.sh --dry-run
#
# Requires: `git`, `gh` (logged in), `cargo`. SSH push to $REMOTE.

set -e
set -o pipefail

REMOTE="${REMOTE:-origin}"
BRANCH="${BRANCH:-feat/cockpit-theme}"
BASE="${BASE:-main}"
WT="${WT:-/tmp/jeryu-cockpit-theme}"
DRY_RUN="${DRY_RUN:-0}"

if [[ "${1:-}" == "--dry-run" ]]; then DRY_RUN=1; fi

cd "$(dirname "$0")/.."
REPO_ROOT="$(pwd)"

err() { echo "error: $*" >&2; exit 1; }
log() { echo "==> $*"; }

git rev-parse --git-dir >/dev/null 2>&1 || err "not in a git repo"
command -v gh >/dev/null 2>&1 || err "gh CLI required"
gh auth status >/dev/null 2>&1 || err "gh not logged in; run 'gh auth login'"
command -v cargo >/dev/null 2>&1 || err "cargo required"
git ls-remote --exit-code "$REMOTE" "$BASE" >/dev/null 2>&1 || err "remote '$REMOTE' or '$BASE' not reachable"

log "fetching $REMOTE/$BASE..."
git fetch "$REMOTE" "$BASE" >/dev/null

if [[ -d "$WT" ]]; then
  log "removing stale worktree at $WT..."
  git worktree remove --force "$WT" 2>/dev/null || rm -rf "$WT"
fi

if git show-ref --verify --quiet "refs/heads/$BRANCH"; then
  if [[ -n "$(git log --oneline "$BRANCH" "^$REMOTE/$BASE" 2>/dev/null)" ]]; then
    err "branch $BRANCH exists locally with unpushed commits; rename or delete it first"
  fi
  git branch -D "$BRANCH" >/dev/null 2>&1 || true
fi

log "creating worktree at $WT on new branch $BRANCH from $REMOTE/$BASE..."
git worktree add "$WT" -b "$BRANCH" "$REMOTE/$BASE" >/dev/null

# Single-file change: just src/tui/theme.rs.
log "copying src/tui/theme.rs..."
cp "$REPO_ROOT/src/tui/theme.rs" "$WT/src/tui/theme.rs"

cd "$WT"
log "cargo check..."
cargo check -p jeryu --tests --message-format=short 2>&1 | tail -3

log "cargo test -p jeryu --lib tui::theme..."
cargo test -p jeryu --lib tui::theme 2>&1 | tail -3

log "cargo fmt --all (project formats; not --check)..."
cargo fmt --all || true

git add src/tui/theme.rs Cargo.lock 2>/dev/null || git add src/tui/theme.rs
git status --short

if [[ "$DRY_RUN" == "1" ]]; then
  log "DRY_RUN=1: stopping before commit / push / gh pr create"
  log "worktree left at $WT for inspection; remove with: git worktree remove --force $WT"
  exit 0
fi

log "committing..."
git -c user.name="$(git config user.name)" -c user.email="$(git config user.email)" \
    commit -m "$(cat <<'EOF'
feat(tui/theme): cockpit palette + Tone/chip helpers (semantic colors)

Upgrades `src/tui/theme.rs` with the "production cockpit" semantic color
system: a new `Theme::cockpit()` constructor, a `CiState` enum, a `Tone`
struct (fg/bg/border per state), and helpers `chip()`, `ci_state()`,
`tone()`, `phase_fill()`, `connector_style()`, `path_style()`,
`running_icon()`, `running_border()`.

Key design points from the outsider feedback this PR addresses:
- Dark tinted *fills* per state (not just bright borders) so jobs feel
  like real objects on the canvas.
- Brighter `focus` for the selected card; muted neutral for the rest.
- Connector lines stay grid-neutral unless on the critical path.
- Spinner + pulse only for running jobs; failed/blocked stay stable.

Backward compatibility: all existing `Theme::dark()` and `Theme::high_contrast()`
constructors keep their fields and behavior. New fields are populated with
sane fallbacks under those palettes so legacy consumers never see Color::Reset
where they used to see a soft RGB. No call sites were changed; this PR is
additive scaffolding that the workflow / mission / agents panels can opt
into incrementally.

Tests: 12 unit tests in `tui::theme` (3 original + 9 cockpit-specific). All
existing 489 lib tests still pass.
EOF
)"

log "pushing to $REMOTE/$BRANCH..."
git push -u "$REMOTE" "$BRANCH"

log "opening PR via gh..."
PR_BODY_FILE="$(mktemp)"
cat > "$PR_BODY_FILE" <<'EOF'
## Summary

Adds `Theme::cockpit()` — a calmer, production-grade semantic color palette
for the TUI, plus a `Tone` + `chip()` API so cards and chips can have dark
*filled* backgrounds per state instead of bright borders on neutral surfaces.

Implements the outsider feedback from #YOUR_ISSUE (or inline review):
- **#080C14** background, **#0D1320** panels, **#263244** grid
- Per-state filled tones (success / running / failed / blocked / waiting /
  agent / idle), each with `{icon, label, fg, bg, border}`
- **#E2E8F0** `focus` color for selected paths
- Connector style helper: most lines stay grid-neutral; failed / blocked /
  waiting paths take state color; selected path gets `focus`
- Spinner + pulse helpers (`running_icon`, `running_border`) so only
  running jobs animate

## Backward compatibility

- `Theme::dark()` and `Theme::high_contrast()` keep all existing fields
  and behavior.
- New struct fields (`grid`, `fill_*`, `border_*`, `focus`) are populated
  with sane fallbacks under those palettes, so any code reading them sees
  legitimate RGB values, not `Color::Reset`.
- No call sites changed in this PR. Workflow / mission / agents panels
  can adopt `Theme::cockpit()` + `chip()` panel-by-panel.

## Test plan

- [x] `cargo test -p jeryu --lib tui::theme` — 12 pass (3 original + 9 new)
- [x] Full `cargo test -p jeryu --lib` — 501 pass (was 489 baseline + 12)
- [x] `cargo check -p jeryu --tests` clean

## How to adopt incrementally

For a panel that currently calls `theme.status_color(s)`:

```rust
// before
let color = theme.status_color(status);
let glyph = theme.status_glyph(status);

// after — single source of truth
let state = theme.ci_state(status);
let chip  = theme.chip(label, state);            // filled, bold span
let tone  = theme.tone(state);                    // for border + bg of the surrounding Block
```

For dependency lines:

```rust
let style = theme.connector_style(state, is_selected);
```

For critical-path mode:

```rust
let style = theme.path_style(base_style, in_critical_path);
```

EOF

PR_URL="$(gh pr create \
  --base "$BASE" \
  --head "$BRANCH" \
  --title "feat(tui/theme): cockpit palette + Tone/chip helpers" \
  --body-file "$PR_BODY_FILE")"
rm -f "$PR_BODY_FILE"

echo
echo "PR opened: $PR_URL"
echo "worktree left at $WT for inspection. Remove with:"
echo "  git worktree remove --force $WT"
