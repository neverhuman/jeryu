# jeryu-wsversion Agent Guidance

Owns:
- Workspace version decisions from conventional commits and public API signals.
- The single `[workspace.package].version` source of truth.
- Changelog roll-forward for release commits.
- The inherit guard that requires every workspace member to use
  `version.workspace = true`.

Forbidden:
- Manual per-crate version pins or ad hoc version bumps.
- Git commits, pushes, or tags from the versioning binary.
- Silent fallback when git ranges, workspace manifests, changelog headings, or
  public API probes cannot be read.

Proof lane:
- `cargo test -p jeryu-wsversion --jobs 40`
- `cargo run -q -p jeryu-wsversion -- inherit-guard`
- `cargo run -q -p jeryu-wsversion -- decide --range origin/main..HEAD --json`
