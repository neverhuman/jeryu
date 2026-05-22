#!/usr/bin/env bash
# Refresh README-visible TUI media from deterministic demo data.
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

capture_tab() {
  local tab="$1"
  local output="$2"
  cargo run --release -p jeryu -- tui \
    --capture \
    --tab "$tab" \
    --output "$output" \
    --width "${JERYU_TUI_MEDIA_COLS:-160}" \
    --height "${JERYU_TUI_MEDIA_ROWS:-44}"
}

capture_tab workflow "$OUT_DIR/tui-workflow.png"
capture_tab jobs "$OUT_DIR/tui-jobs.png"
capture_tab bugs "$OUT_DIR/tui-bugs.png"

JERYU_TUI_RECORDING_OUT="$OUT_DIR/tui-demo.gif" \
  cargo test --test tui_recording -- --ignored --exact tui_demo_recording

cp "$OUT_DIR/tui-demo.gif" "$ASSET_DIR/tui-demo.gif"
cp "$OUT_DIR/tui-workflow.png" "$ASSET_DIR/tui-workflow.png"
cp "$OUT_DIR/tui-jobs.png" "$ASSET_DIR/tui-jobs.png"
cp "$OUT_DIR/tui-bugs.png" "$ASSET_DIR/tui-bugs.png"

ls -lh \
  "$ASSET_DIR/tui-demo.gif" \
  "$ASSET_DIR/tui-workflow.png" \
  "$ASSET_DIR/tui-jobs.png" \
  "$ASSET_DIR/tui-bugs.png"
