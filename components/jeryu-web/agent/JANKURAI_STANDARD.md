# Jeryu Split Repo Standard

Source commit: `cbecf7caa0e932c76a341b2521e66e911233860d`
Split repo: `jeryu-web`
Required check: `jeryu-web/required`

Required local commands are `just fast`, `just check`, `just score`, and
`just security`. Release-supporting repos also expose `just artifact-support`.

Score files under `.jankurai/` are produced by the pinned Jankurai lane.
Do not hand-edit them.
