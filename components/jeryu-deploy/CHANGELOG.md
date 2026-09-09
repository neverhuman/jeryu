# Changelog

## Unreleased

- Decompose the agent-run web control surface into handler, bounded-store,
  frozen-diff export, and focused TTY regression modules; preserve the existing
  route and wire behavior behind a new narrow `just agent-runs` proof command.
- Split the remaining oversized API and split-tool sources at their existing
  catalog, bootstrap, session-runtime, Git-source, pull-posture, installed-audit,
  and test boundaries without changing their public paths or wire contracts.
- Propagate a bounded `x-request-id` across both HTTP and MCP responses, replacing
  hostile or oversized caller values and covering the boundary with focused tests.
- Make `tools/security-lane.sh` the canonical executable security authority,
  keep the historical ops path as a compatibility delegate, and require full
  Cargo/npm dependency audits from the comprehensive and PR validation lanes.
- Reserve external check-run, commit-status, and Jankurai score publication for
  global-admin maintenance while native runner results remain server-published.
- Require repository-admin authority for branch-protection changes instead of
  allowing any repository writer to weaken the evidence gate.
- Recompute push-time Jankurai evidence before adopting stored state so an
  interrupted or tool-failed audit can recover on the same commit; reject
  incomplete/nonzero tool output, audit a first main ref against an empty tree,
  and prevent candidate policy from lowering the host score floor.
- Include the security lane in the canonical required-check entrypoint and keep
  its workflow declaration aligned with the commands the lane actually runs.
- Repair the standalone full and affected CI paths so they use repository-owned
  proof, workflow, release-receipt, score, and security gates instead of absent
  monorepo packages, and validate owner/test coverage from the tracked tree.
- Make all ten phase gates exercise only real owned integrations or resolvable
  immutable dependencies, validate the vendored web bundle without npm source,
  and stop the PR gate from silently restoring a changed `Cargo.lock`.
- Rebind the API coverage floor once from unreproducible pre-split `0.8411` to
  `0.8044`: exact hosted protected main measured `0.7981`, while this candidate
  improves it to `0.8044`; all later baseline updates remain upward-only.
- Bound coverage-test concurrency independently from compiler concurrency so
  process-heavy identity and live-route tests do not fail under host pressure.
- Apply the same eight-process default to aggregate CI tests and stabilize the
  fsynced executable test fixture with a bounded Linux `ETXTBSY` readiness
  check without retrying or weakening the production identity verifier.
- Keep deleted files in the protected-base proof plan while passing only the
  exact extant changed-path subset to Jankurai 1.6.11 proofbind, and assert both
  scopes so stale evidence removal cannot be mistaken for a missing input.
- Replace duplicate proof scripts and their swallowed failures, candidate
  self-baseline, and synthesized UX/migration/vibe/coverage outputs with one
  strict standalone lane backed by a provenance-bound hosted-main baseline;
  make source-security receipts name only commands that actually ran.
- Supply Jankurai's canonical `tools/security-lane.sh` entrypoint as a governed
  delegate to the real standalone security implementation, with explicit owner
  and test-map coverage.
- Add locked, package-scoped API check/test commands and an explicit sccache
  status probe for fast deterministic developer feedback, and make every
  canonical Rust CI/build entrypoint refuse dependency-lock drift.
- Patch the locked `anyhow` and `h2` advisories, keep Cargo Deny default-deny,
  and route every historical Cargo Git identity through exact
  `git.neverhuman.org` mappings and immutable hosted support refs with
  fresh-cache and hostile regression proof.
- Require CI verification and publication to use the one exact hosted origin
  with no alternate push URL, while allowing the accepted predecessor runtime
  to stay live during candidate tests that select freshly built binaries.

## jeryu-deploy-v5.0.0-split.4

- Bind pull-request reviews and merge protection to the exact current head via
  immutable Jeryu Core split.5, preserving historical reviews as stale audit
  evidence and applying latest-review-per-reviewer precedence.
- Bind hosted MCP and agent entry points to authenticated principals rather
  than request-supplied actor names.
- Rotate authenticated sessions after password changes so the old auth epoch is
  revoked while the caller receives a fresh cookie and CSRF token.
- Isolate live Git LFS transport tests from user and system Git configuration so
  global filter hooks cannot mutate or falsely fail the disposable repository.

## jeryu-deploy-v5.0.0-split.3

- Decode bounded gzip/x-gzip Git smart-HTTP pack RPC requests before invoking
  Git, reject malformed or stacked encodings, and preserve protocol-v2 headers.
- Govern every active Jankurai consumer on the local-authority 1.6.11 split.2 tag and
  exact binary digest and installation receipt, with physical-file,
  wrong-authority, hostile-substitution, and PATH-neutralization tests plus
  root-broker release-custody controls; retain the 1.6.10 score baseline only as
  non-authoritative history.

## jeryu-deploy-v5.0.0-split.0 - 2026-06-11
- MAJOR: first standalone split-family release; the legacy monorepo
  (/home/ubuntu/jeryu) is deprecated and its drift fully reconciled.

## jeryu-deploy-v4.0.0-split.0

- Initial split-family baseline for `jeryu-deploy`.
