# Contributing to Jeryu

Open issues and pull requests at [neverhuman/jeryu](https://github.com/neverhuman/jeryu).
The root Rust and npm workspaces are the development entrypoint. Edit the owning
component under `components/`; standalone repositories are downstream mirror
targets. Work keeps its `jeryu-jira` package and repository identity.

Read [AGENTS.md](AGENTS.md), the owning component's guidance and
[architecture](docs/architecture.md) before changing source. Preserve public
interfaces, package names, version distinctions, Apache-2.0 licensing and
third-party notices. Generate contracts from their owning Rust packages.

Start with [README.md](README.md) to build the application. Use
[testing](docs/testing.md) to select the affected owning commands. Add
behavior-focused regression coverage for defects, including failure and
authorization cases where relevant. Keep evidence tied to the exact source
revision; report failed, skipped or unavailable checks explicitly.

A pull request should explain the user-visible problem, the resulting behavior
and the commands actually executed. Include source revisions and evidence links
for audit, runtime, export or release claims. Policy changes must preserve
stronger governing floors and ratchets; the common minimum is 85, with zero
hard findings and caps.

Keep branches linear. Preserve existing ancestry and immutable tags. Never
force-push, move tags, create Git worktrees, use copied authoring checkouts or
overwrite an independently changed mirror. Local canonical checkout claims and
stopped-head handoffs remain required where applicable.

Independent review and the real required checks precede protected merge.
Author, reviewer and merger must remain distinct. A source merge does not
activate an installed service or qualify a release.

Keep credentials, runtime exports, `.work`, custody archives and build output
out of commits and issue attachments. Use [SECURITY.md](SECURITY.md) for
vulnerabilities and [SUPPORT.md](SUPPORT.md) for help.
