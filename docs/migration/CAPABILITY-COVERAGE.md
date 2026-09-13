# Command and capability coverage

Status: **PENDING qualification**. This inventory combines source inspection
with selected working-tree verification on 2026-09-10, based on commit
`306171230bbdffcf643242ddd482df12ad1852ae`. Those tests preceded the final
candidate and do not qualify that commit or a subsequent release. A named test
remains an obligation at the final clean source. [Current status](STATUS.md)
owns release admission.

## Production command surface

The [CLI grammar][cli] defines help. Production `main.rs` always selects an
HTTP API URL and uses `RemoteOnlyClient` for commands without an HTTP adapter.
That client returns an explicit unavailable error, mapped to exit 5; it does not
provide a temporary in-memory forge. Several older dispatch tests deliberately
use `InMemoryForgeClient` and therefore do not establish production capability.

| Command or browser operation | Implemented transport and storage | Existing proof and remaining boundary |
| --- | --- | --- |
| `serve`; first login; password change; personal tokens | Embedded SPA, authenticated API, SQLite under the selected data directory. `--data-dir`, `JERYU_DATA_DIR`, then XDG/default selection; explicit `--spa-dir` for development. | [Standalone tests][standalone]: first-start credential restrictions, password change, token use, data-directory selection and restart. Anonymous public-origin installation remains pending. |
| `forge repo create/list`; browser preview/create | CLI uses `/repos`; browser uses `/api/v1/repos/preview` and `/api/v1/repos`. Core SQLite plus managed bare Git storage; browser creation has persisted idempotency receipts. | `startup_cli_and_restart_use_durable_state_from_any_directory`; API `repository_create_tests`; [real browser smoke][browser-live] previews and creates a private repository, then reloads. Interrupted multi-store creation and operator recovery need an executed drill. |
| Git push and clone | Smart HTTP under `/git/`; public reads, authenticated writes/private reads, managed Git objects and protected refs. | Selected standalone process tests passed public anonymous clone, invalid-token, private-read and anonymous-write controls. Actual API router tests bind permission checks to the resolved Git storage identity and prevent invalid explicit credentials from borrowing a session. Anonymous LFS download is not established. `jeryu-gitd` [HTTP authorization tests][git-auth] cover lower-level denials. |
| `forge issue create/list`; repository Issues/Work views | CLI GitHub-shaped issue API, durable core store and Work bridge. Work has its own `work.sqlite`. | CLI issue survives process restart in standalone tests. API `github_issue_create_is_mirrored_into_work` covers bridge behavior; it calls the adapter directly. Browser Work uses mocked HTTP. |
| `forge pr open/list/status/merge`; browser reviews and merge | CLI live HTTP; GitHub-shaped PR endpoints and BFF review/check/merge routes. Core stores reviews, grants, protections and checks; merge updates real Git refs. | `authenticated_protected_review_checks_merge_and_restart_preserve_exact_head` uses real HTTP, distinct actors, denials, exact-head evidence and two restarts. Its check results are disposable fixture evidence, not CI qualification. CLI HTTP merge has a fixture test; the complete PR lifecycle through CLI and browser remains pending. |
| `ci status` | Live, authenticated paginated check-run reads. Empty evidence remains empty; malformed, duplicate or incomplete pages are errors. | [CI HTTP tests][ci-http] plus `ci_status_reads_authorized_check_evidence_across_restart` cover denials, missing repository, restart, output and failure states. This reads checks; it does not execute an audit or certify a release. |
| Work list/create/edit/comments/links | `/api/v1/work` and repository-scoped aliases use `jeryu-jira::WorkStore`. No dedicated Work CLI exists. Unbound Work is administrative; repository Work follows repository grants. | Six actual API router scenarios passed permissions, authenticated actors, denial side effects, SQLite reopening and repository identity checks. The owning Work suite passed a deterministic concurrent-read snapshot case. A complete production-process/browser Work journey and atomic recovery across the separate Core and Work stores remain open. |
| `status`, `priorities`, `repo-graph clusters`, `artifacts latest`, `runners status` | HTTP control-plane reads; administrative gate. Evidence views can correctly report unknown or absent capabilities. | Standalone tests assert an unconfigured runner stays `unknown` with zero capacity across restart. Other complete installed CLI scenarios remain pending. |
| `tool-finder clusters/summary` | HTTP codegraph candidates and tool registry reads. | Adapter source exists; the installed CLI's success/denial/unavailable-service matrix is not established by the standalone smoke suite. |
| `agent run/status/control/follow/export-pr` | HTTP agent-run routes. Optional runtime depends on actual runner/workcell and isolation capabilities. | API agent-run/workcell tests exist. Neither a passing source probe nor ordinary SQLite installation qualifies the optional product image. |
| `gh-setup`, `autonomy init`, `onboard --dry-run` | Local configuration generation or an onboarding rehearsal; these are not remote forge mutations. | `cli_snapshots.rs` tests print/write behavior and dry-run output. `onboard` without `--dry-run` returns unavailable. |

Account administration, grants, protection configuration, check publication and
review operations have API routes but no corresponding dedicated CLI commands.
The legacy `forge pr merge --trust-tier` option now accepts only its default
`trusted` compatibility value. Other values fail before a request is sent;
server policy decides merge eligibility. Selected request-capture regressions
passed; the final clean candidate must repeat its owning lane.

## Explicitly unavailable CLI operations

These help-visible leaves have no production transport:

- `ci run`, `ci explain`.
- `runner list`, `runner enroll`, `runner drain`, `runner rotate`.
- `proof verify`, `proof explain`, `release --version`, `cache self-test`.
- `agent auth import`, `agent auth doctor`; onboarding without `--dry-run`.

`unreachable_api_and_unimplemented_operations_cannot_report_success` now covers
an unreachable repository request, all twelve unavailable leaves and onboarding
without `--dry-run`. It requires exit 5, empty success output, a transport error
and no newly created state for each unavailable operation. The expanded test
passed in the selected unprivileged process run recorded in current status.
In-memory dispatch successes are separate coverage.
Command help labels these operations unavailable and identifies onboarding as
a dry-run operation.

## Browser controls and proof scope

At the inspected baseline, [CreateRepoForm][create-form] enables `local` hosting,
`internal` visibility and topics. [Repository validation][create-api] rejects
all three; it supports only `jeryu`, public/private visibility and empty topics.
Template, gitignore and license-template request fields are also unsupported;
the current dialog sends them as null and does not expose template controls.

The current form correction disables the unsupported host and
visibility options, labels them unavailable, and disables Topics with an
explicit unavailable label. Public/private selection and the existing private
creation journey remain covered by amended existing Playwright scenarios.
Selected rendered scenarios and the real-backend creation scenario passed,
with retained screenshots inspected. The later Work-page wording and link-style
change also passed its selected rendered replay in the acquisition/recovery
slice. The complete web lane remains open.

The [mocked repository scenario][browser-repos] retains its screenshot and
checks available selections. The Work action matrix in `e2e/24-work-tracker.spec.ts`
intercepts mutation requests; it verifies UI behavior, not server persistence
or authorization. The real BFF `standalone.spec.ts` covers login, session reuse,
preview and repository creation. It does not cover the complete browser issue,
Work, protected PR, upgrade or recovery journey.

## Owning commands and open acceptance

Run from the monorepo root, with the required tool preparation:

| Command | What it establishes when freshly successful |
| --- | --- |
| `bash scripts/ci.sh runtime` | Builds production assets/binary, then runs source-install verification and the seven installed-binary standalone scenarios. |
| `bash scripts/ci.sh web` | Locked frontend checks, contracts, production build, Storybook, real BFF and mocked rendered/action/accessibility obligations. |
| `cargo test --locked -p jeryu-cli --test ci_status_http` | Focused check-status transport, report validation and error behavior. |
| `cargo test --locked -p jeryu-api repository_create_tests` | Read-only preview, receipt replay after reopening, invalid inputs and refusal to adopt orphan Git storage. |
| `cargo test --locked -p jeryu-api pulls_review_routes` | Review authentication, repository access, identity/head binding and failure rollback. |
| `cargo test --locked -p jeryu-jira` | Work storage, properties, migrations and contract tests; not complete HTTP authorization. |

The install test rejects modified binaries, changed source, missing/malformed
receipts and exercises installation transaction recovery. The
[operator guide](../recovery.md) now documents whole-directory stopped backup,
restoration, upgrades, failed creation and remote TLS. The extended standalone
scenario passed same-binary restoration of authentication, Git, issues and
Work plus a post-restore write and restart in the selected acquisition/recovery
run. Cross-version upgrades, actual TLS deployment and interrupted-creation
repair remain open.
Fresh unprivileged empty-cache qualification, anonymous public-origin testing,
and the full local/hosted matrix remain required at the exact release source.

[cli]: ../../components/jeryu-deploy/crates/jeryu-cli/src/cli/mod.rs
[standalone]: ../../components/jeryu-deploy/crates/jeryu-cli/tests/standalone.rs
[ci-http]: ../../components/jeryu-deploy/crates/jeryu-cli/tests/ci_status_http.rs
[git-auth]: ../../components/jeryu-core/crates/jeryu-gitd/tests/http_auth.rs
[browser-live]: ../../components/jeryu-web/apps/web/e2e/standalone.spec.ts
[browser-repos]: ../../components/jeryu-web/apps/web/e2e/02-repos.spec.ts
[create-form]: ../../components/jeryu-web/apps/web/src/components/repo/CreateRepoForm.tsx
[create-api]: ../../components/jeryu-deploy/crates/jeryu-api/src/web/repository_create.rs
