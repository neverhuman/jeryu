# Jeryu Split Repo Standard

Split repo: `jeryu-tool`
Required check: `jeryu-tool/required`
Profile: `public-portal` (manifest/scripts/docs control plane — no product code)

Required local commands are `just fast`, `just check`, `just score`, and
`just security`; this repo also exposes `just artifact-support`. `just` (no
recipe) runs the full local gate in one command, and the GitHub workflow runs
the same lanes so CI and local stay at parity.

`just fast` is the deterministic fast lane: it asserts every family consumer's
jankurai pin still matches `tool-manifest.toml` (the single source of truth).

Generated zones are not hand-edited: `.jankurai/**` is produced by `just score`,
and `generated/**` (the pin env) is produced by `ops/render-tool-manifest.sh`.
