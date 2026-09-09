# Historical Phase 9 Implementation Notes

This file describes the pre-split seed and is not a current workspace map. The
current Deploy checkout owns `jeryu-api`, `jeryu-cli`, and `jeryu-split-tool`;
the family components named below now live in protected standalone repos and
enter through immutable release pins.

This package implements the Phase 9 scope from the supplied Jeryu engineering spec:

- users/orgs/teams/repos
- issues/comments/labels
- pull requests/reviews/review comments
- branch protection
- commit statuses
- check runs/check suites
- webhooks and durable delivery outbox
- GitHub-compatible REST subset

The original fused repository put product truth in `crates/jeryu-core` and the
REST edge in `crates/jeryu-api`. Only the latter source remains in this split.

## What is intentionally deferred

The uploaded spec defines later phases for CI compiler, scheduler, native runners, RustJet, JeryuCache, merge queue, SignRail, and imports. Those systems are not implemented in this Phase 9 tarball except as compatibility boundaries in the API surface.

## Local validation

The historical import environment did not provide `cargo`. Current validation
is executable with:

```bash
just fast
just full
```

on a machine with Rust installed.
