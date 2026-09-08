# Jeryu development instructions

This branch assembles the standalone Jeryu monorepo. Read `README.md` and
`docs/migration/STATUS.md` before changing source or making release claims.
The public development destination is `neverhuman/jeryu`. Changes belong in
`components/<repository>/`; split repositories will receive one-way exports.

Use the root Cargo workspace and lockfile for all 65 Rust packages, and the
root npm workspace and lockfile for the web application and UX tooling.
Internal Jeryu dependencies use `[workspace.dependencies]`. Preserve package
names and the existing 5.0.0 / 5.1.0 version distinctions. Redline remains an
external immutable dependency with its own authority and two-consumer proof.

The root manifest describes the candidate handover. Its `handover.status`
must remain pending until the protected review and qualification gates pass.
The existing released Release Ops authority and installed service are unchanged
by source assembly. A public source candidate does not activate production.

Component guidance retains ownership, contract generators, security policies,
and proof thresholds. Its historical split-only source-routing instructions
apply to standalone exports; they do not prohibit workspace dependencies in
this explicitly authorized monorepo. Generated contracts must come from their
owning Rust packages. Historical manifests and lockfiles under
`docs/migration/original-manifests` are provenance, not active configuration.

Do not create Git worktrees, copied source checkouts, or compatibility symlinks.
Use the claimed canonical checkout. Disposable exact-commit Git clones used
for qualification must be removed by a cleanup guard. Preserve all refs and
untracked material, verify restoration, and obtain an explicit stopped-head
handoff before retiring any original checkout. Local family coordination and
checkout-holder rules continue to apply during the transition.

Keep candidate branches linear and preserve existing GitHub ancestry and
immutable tags. Never force-push, weaken protection, fabricate checks, or
self-review. Author, reviewer, and merger must remain independent. CI must
report unavailable capabilities or skipped required proofs as failures.

Never commit credentials, runtime exports, `.work`, custody bundles, or build
output. Durable standalone state resolves through `--data-dir`,
`JERYU_DATA_DIR`, and XDG storage. Normal builds must eventually work without
personal Git configuration, private forge access, or adjacent repositories;
the migration status records remaining qualification blockers honestly.
