# Jeryu Split Repo Standard

Split repo: `jeryu-jira`
Required check: `jeryu-jira/required`

Required local commands are `just fast`, `just check`, `just score`, and
`just security`. Release-supporting repos also expose `just artifact-support`.

Score files under `.jankurai/` are produced by the pinned Jankurai lane.
Do not hand-edit them.
