# Paper Assets

This directory contains publication figures and generated screenshots.

Generated screenshots for V3.01:

- `jeryu-tui-mission.png`
- `jeryu-tui-release.png`
- `jeryu-tui-jobs-flow.png`
- `jeryu-tui-agents.png`
- `jeryu-tui-tests-vti.png`
- `jeryu-tui-evidence.png`

Use `scripts/capture-tui-screenshots.sh` to render high-resolution, full-terminal
PNG screenshots for publication. The script runs `jeryu tui --screenshot` inside a
real PTY, parses the terminal state with `vt100`, and rasterizes the full grid
with `tui-capture` using a pinned DejaVu Sans Mono font, fixed geometry, and a
brightened paper-friendly palette. This avoids browser/ANSI converter glyph
fallback and captures the alternate screen before the TUI exits.

```bash
./scripts/capture-tui-screenshots.sh
just tui-screenshot-smoke
```

For deterministic, non-interactive CI comparisons, the  deterministic
renderer remains available as:

```bash
jeryu tui --capture --tab <tab> --output paper/assets/<name>.png
```
