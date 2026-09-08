# Jeryu Split Repo Standard

Split repo: `jeryu-tool-finder`
Required check: `jeryu-tool-finder/required`
Profile: `public-portal` (single Rust discovery CLI)

Required local commands are `just check`, `just score`, `just security`,
`just contract-drift`, and `just artifact-support`. `just` (no recipe) runs the
full product gate in one command. Protected hosted CI uses the same scripts.

There is **no `fast` lane** here: the jankurai pin is the single source of truth
in `jeryu-tool`'s `tool-manifest.toml`, not in this repo. The separate Cargo
dependency edge is pinned to immutable Jeryu Intelligence `split.1` and checked
by the locked metadata gate.

The discovery surface (`just scan` / `just dossier` / `just propose`) is the
compiled `jeryu-tool-finder` binary linked to the pinned `jeryu-codegraph`
library. Relative path patches are not permitted.

Generated zones are not hand-edited: `.jankurai/**` is produced by `just score`,
and `dossiers/**` is produced by `just scan` + `just dossier`.
