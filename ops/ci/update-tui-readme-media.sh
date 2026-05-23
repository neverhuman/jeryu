#!/usr/bin/env bash
# Refresh README-visible TUI media from deterministic demo data.
#
# Uses tuiwright's TerminalRenderer (embedded JetBrains Mono TTF) for all
# outputs, producing crisp, font-rendered PNGs and a high-quality animated GIF.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

OUT_DIR="${JERYU_TUI_MEDIA_OUT_DIR:-target/ci-screenshots}"
ASSET_DIR="${JERYU_TUI_MEDIA_ASSET_DIR:-assets}"
mkdir -p "$OUT_DIR" "$ASSET_DIR"

export TERM="${TERM:-xterm-256color}"
export COLORTERM="${COLORTERM:-truecolor}"
export JERYU_DATABASE_URL="${JERYU_DATABASE_URL:-sqlite::memory:}"
export JERYU_TUI_WORKFLOW_INSPECT_OPEN="${JERYU_TUI_WORKFLOW_INSPECT_OPEN:-1}"

cargo build --release -p jeryu

# Capture font-rendered PNG screenshots via tuiwright's TerminalRenderer.
# This produces crisp, readable PNGs with embedded JetBrains Mono TTF — not
# the pixel-block rectangles from the old `jeryu tui --capture` path.
JERYU_TUI_SCREENSHOTS_DIR="$OUT_DIR" \
  cargo test --release --test tui_recording -- --ignored --exact tui_readme_screenshots

# Record the animated demo GIF (also via tuiwright's renderer).
JERYU_TUI_RECORDING_OUT="$OUT_DIR/tui-demo.gif" \
  cargo test --release --test tui_recording -- --ignored --exact tui_demo_recording

cp "$OUT_DIR/tui-demo.gif" "$ASSET_DIR/tui-demo.gif"
cp "$OUT_DIR/tui-workflow.png" "$ASSET_DIR/tui-workflow.png"
cp "$OUT_DIR/tui-jobs.png" "$ASSET_DIR/tui-jobs.png"
cp "$OUT_DIR/tui-bugs.png" "$ASSET_DIR/tui-bugs.png"

ls -lh \
  "$ASSET_DIR/tui-demo.gif" \
  "$ASSET_DIR/tui-workflow.png" \
  "$ASSET_DIR/tui-jobs.png" \
  "$ASSET_DIR/tui-bugs.png"
