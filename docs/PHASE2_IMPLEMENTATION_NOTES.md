# Phase 9 Implementation Notes

This package implements the Phase 9 scope from the supplied Jeryu engineering spec:

- users/orgs/teams/repos
- issues/comments/labels
- pull requests/reviews/review comments
- branch protection
- commit statuses
- check runs/check suites
- webhooks and durable delivery outbox
- GitHub-compatible REST subset

The repository is organized as a Rust workspace with product truth in `crates/jeryu-core` and the REST edge in `crates/jeryu-api`.

## What is intentionally deferred

The uploaded spec defines later phases for CI compiler, scheduler, native runners, RustJet, JeryuCache, merge queue, SignRail, and imports. Those systems are not implemented in this Phase 9 tarball except as compatibility boundaries in the API surface.

## Local validation

This environment did not provide the `cargo` executable, so the package could not be compiled here. The code is arranged to be validated with:

```bash
just fast
just full
```

on a machine with Rust installed.
