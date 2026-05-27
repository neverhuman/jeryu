# JeRyu Web Forge — Roadmap

Release milestones for the Web Forge SPA + BFF. Calendar estimates assume
4 parallel agents executing per WEB_WORK_CLAUDE.md §6 claim protocol.

For the wider product roadmap (TUI, agent runtime, CI, host adapters)
see [`docs/MISSION.md`](docs/MISSION.md) and the existing planning files
in `tips/phases/`. This document is **scoped to the Web Forge**.

| Milestone | Window | Scope |
|---|---|---|
| **v0.1 Alpha** | Week 1 | Foundations and bootstrap. |
| **v0.3 Repos + README** | Week 2 | Repo list, create, README render. |
| **v0.5 Beta** | Week 3–4 | Code browser + Merge cockpit. |
| **v0.8 RC** | Week 5 | Settings + CI/activity streaming. |
| **v1.0** | Week 6 | Hardening, accessibility, docs. |
| **v1.1** | Week 7–8 | Polish — shortcuts, filters, density modes. |
| **v1.5** | Month 2–3 | Issues, agents UI, GitHub adapter parity, Mermaid (flagged). |
| **v2.0** | Quarter 2 | Plugins, custom dashboards, multi-tenant, mobile responsive. |

---

## v0.1 Alpha — Week 1

Phase 0 + Phase 1 from WEB_WORK_CLAUDE.md §8. Goal: a developer can open
`http://127.0.0.1:5173` and see the app shell with real bootstrap data
plus a live WebSocket indicator.

Scope:

- `apps/web` workspace stands up as `@jeryu/web` (Vite + React + TS).
- Axum BFF binds `127.0.0.1:8787`; `/api/v1/bootstrap` returns
  `WebBootstrap` with viewer + permissions + recent_repos snapshot.
- `GET /api/v1/ws` upgrade works; SPA shows `connected` after `hello`.
- App shell, global header, left nav, status bar, command palette,
  live activity dock render.
- Design tokens (light/dark/high-contrast) shipped.
- React Router 6 route table for every page (most are stub pages until
  later milestones).
- ts-rs / utoipa / schemars generators in place; CI fails on drift.
- Engine routes `/health`, `/hooks`, `/cache/summary` preserved.

Exit criteria:

- `cargo check --workspace --features web` green.
- `npm run build` green; bundle <350 KB gz initial shell.
- Storybook builds; addon-a11y configured.
- `jeryu web serve` stub responds with real bootstrap.

---

## v0.3 Repos + README — Week 2

Phase 2 from WEB_WORK_CLAUDE.md §8. Goal: a user can list all internal
GitLab repos, create a repo via preview/execute, and open any repo to
see a correctly rendered sanitized README.

Scope:

- GitLab host adapter (`src/git_host/gitlab.rs`) wired for list, get,
  create, archive, delete repo operations.
- `RepoService` + `RepoBrowserService` reading from the cache + sync
  background task.
- `POST /api/v1/repos/preview` and `POST /api/v1/repos`, both with
  `Idempotency-Key`.
- `GET /api/v1/repos/{repo_id}/readme` returns `BlobResponse` with
  `rendered_markdown`.
- `POST /api/v1/markdown/render` exposed (§35.1.8).
- Renderer pipeline (`pulldown-cmark` → `ammonia` → DOMPurify in SPA)
  shipped with `RENDERER_VERSION` + `SANITIZER_VERSION` constants and
  the dual-versioned cache key.
- XSS corpus (`tests/web_markdown_tests.rs`, 21 fixtures) green.
- `RepositoriesPage` and `RepositoryOverviewPage` ship in the SPA.

Exit criteria:

- Repo list shows live data from internal GitLab.
- Creating a repo writes an audit row, a `web_action_receipts` row, and
  emits `repo.created` on `global.activity`.
- Every README XSS fixture is defanged; cache hit latency <25 ms.

---

## v0.5 Beta — Week 3–4

Phase 3 + Phase 4 from WEB_WORK_CLAUDE.md §8. Goal: full code-browsing
and merge-review workflow including exact-SHA approve/merge.

Scope:

- Tree / blob / raw / history / blame surfaces (W-B-10).
- Monaco lazy-loaded on file view route with syntax highlighting.
- Fuzzy-find files via `t` keypress.
- Diff viewer with TanStack Virtual for 20k changed lines.
- Inline comments + threaded discussions per file.
- Merge cockpit with three-pane layout (see
  [`docs/REVIEW_COCKPIT.md`](docs/REVIEW_COCKPIT.md)).
- Merge Passport with all 12 gates (§35.2.4) and stable blocker codes.
- Exact-SHA approve/merge handlers with `expected_head_sha` +
  `409 merge_sha_stale` recovery flow.
- `mr.review.submitted`, `mr.approved`, `mr.merged`, `mr.merge.blocked`
  emitted on `mr.{mr_id}` (high priority).
- Playwright specs for merge-review workflow.

Exit criteria:

- A reviewer can browse, comment, approve at an exact SHA, and merge
  via the SPA without leaving the cockpit.
- Stale-SHA recovery has been tested end-to-end (W-T-14).
- Bundle remains <350 KB gz initial shell; route chunks <80 KB gz.

---

## v0.8 RC — Week 5

Phase 5 + Phase 6 from WEB_WORK_CLAUDE.md §8. Goal: settings management
with blast-radius preview and the live activity stream.

Scope:

- Settings page (`RepositorySettingsPage`) with searchable
  sub-sections.
- `POST /api/v1/repos/{repo_id}/settings/preview` returns
  `SettingsDiffPreview`.
- `PATCH /api/v1/repos/{repo_id}/settings` with `Idempotency-Key` +
  `If-Match`; `409 settings_hash_stale` recovery in SPA.
- Branch protection + members + secrets routes wired.
- CI surfaces — pipelines, jobs, log streaming via `job.log.chunk`
  events (low priority class).
- Live activity dock streams events from all subscribed scopes with
  filtering and acknowledgment.
- Agent evidence read-only tab in the merge cockpit (W-B-31 v1).

Exit criteria:

- Operators can change every settings section through the SPA with
  preview + audit + WS event.
- CI events stream into the cockpit and the activity dock without
  manual refresh.

---

## v1.0 — Week 6

Phase 7 hardening from WEB_WORK_CLAUDE.md §8. Goal: all 16 acceptance
criteria green; the docs you are reading exist.

Scope:

- UX-QA receipts for loading / empty / error / success / permission-
  denied on every major page.
- Accessibility: axe-core zero serious/critical findings; geometry
  checks (44×44 hit targets) pass.
- Lighthouse performance ≥90.
- Performance budgets met: initial shell ≤350 KB gz; FUP ≤1.5 s; route
  transition ≤100 ms; WS p95 ≤250 ms; markdown cache hit ≤25 ms.
- Docs complete: `docs/web-forge.md`, `docs/WEB_API.md`,
  `docs/WEBSOCKET_PROTOCOL.md`, `docs/README_RENDERING.md`,
  `docs/REVIEW_COCKPIT.md`, `apps/web/README.md`, this ROADMAP.
- Deployment artifacts shipped: systemd unit, Dockerfile, nginx
  reference config (WEB_WORK_CLAUDE.md §26).
- Operator runbook (WEB_WORK_CLAUDE.md §27) cross-linked.
- All 16 acceptance criteria in WEB_WORK_CLAUDE.md §9 green.
- All Rust + frontend CI lanes green.

---

## v1.1 — Week 7–8

Polish iteration. No new surfaces; pure refinement.

Scope:

- Additional keyboard shortcuts (the `?` overlay reflects the new set).
- Saved searches and advanced filters across Repos, MRs, Issues, Audit.
- Density modes — compact / regular / comfortable, persisted per user
  via `preferencesStore`.
- Tighter CSP (drop `'unsafe-inline'` on `style-src` via nonces).
- Image proxy for absolute `https://` image URLs (referrer scrubbing).
- Sanitizer policy review; bump `SANITIZER_VERSION` if tightened.
- WS bus split — separate priority + best-effort channels rather than
  a single ranked channel.

---

## v1.5 — Month 2–3

New surfaces that were deliberately out of v1.

Scope:

- **Issues** — full create / update / labels / milestones; `POST
  /api/v1/repos/{repo_id}/issues` no longer returns `501`.
- **Agents UI** — write surface: start a session, attach receipts,
  request evidence packs. `agents.write` perm exercised.
- **GitHub adapter parity** (W-H-07) — the `github.rs` stub is replaced
  with a full implementation matching the `GitLabClient` surface.
- **Mermaid diagrams** behind a feature flag, sandboxed via
  `iframe sandbox="allow-scripts"` in a dedicated `data:`-blocked
  origin.
- **`README.rst`** rendering (currently download-only).
- **Wiki** read surface (no edit yet).
- **Activity dock filters** by event kind and severity.

---

## v2.0 — Quarter 2

Larger, multi-quarter investments.

Scope:

- **Plugins** — declarative panels that ship as separate Vite bundles
  loaded via dynamic import; permissioned by `plugins.read/write`.
- **Custom dashboards** — drag-and-drop tiles backed by saved REST
  queries and WS subscriptions.
- **Multi-tenant SaaS hardening** — tenant isolation in the BFF, signed
  upstream-fetch credentials, per-tenant rate limits.
- **Mobile-first responsive layout** for the diff viewer and merge
  cockpit (current v1 is desktop-only on those pages).
- **Browser-IDE proximate features** — inline code editing for trivial
  fixes ("commit suggestion") without leaving the cockpit.
- **Full enterprise SSO/OIDC** beyond the v1 session-cookie scaffold.

---

## Out of scope

These remain non-goals (WEB_WORK_CLAUDE.md §1.3 + §35.3.4):

- Replacing the Git wire protocol itself.
- Full package registry UI parity (GitHub Packages, GitLab Registry).
- Browser IDE / Codespaces clone.
- Public multi-tenant SaaS hardening at v1 (lands at v2.0).
- GitHub Pages / GitLab Pages hosting from the web app.
- Discussions tab clone.
- Marketplace / apps directory beyond installed integration metadata.
- TUI replacement — the TUI and Web Forge share the read-model
  surface; both ship.

---

## How milestones are tracked

- **Work packages** live in `WEB_WORK_CLAUDE.md` §7 with explicit
  per-package Definition of Done (§13).
- **Claims** are recorded in `tips/web/CLAIMS.md` per the §6 claim
  protocol; an agent picks up a package, posts a claim row, and updates
  it through the lifecycle.
- **CI gates** for each milestone live in `agent/proof-lanes.toml` and
  `agent/ux-qa.toml`.
- **Schemas / contracts** are regenerated on every release and the diff
  is reviewed before tagging:
  - `cargo run --bin jeryu_export_types`
  - `cargo run --bin jeryu_export_schemas`

The full verification gate for v1.0 is in WEB_WORK_CLAUDE.md §19; tag
on a `web-vX.Y.Z` ref when every step is green.
