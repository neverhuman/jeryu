# `@jeryu/web`

Vite + React + TypeScript SPA for the JeRyu Web Forge. This README is the
day-to-day contributor guide; for the architectural picture see
[`docs/web-forge.md`](../../docs/web-forge.md) and for the REST surface see
[`docs/WEB_API.md`](../../docs/WEB_API.md).

- **Source plan:** [`WEB_WORK_CLAUDE.md`](../../WEB_WORK_CLAUDE.md)
  (authoritative).
- **Companion frontend dir:** `apps/ux-qa/` (legacy UX-QA harness, owned
  by W-F-12).
- **Bundle name:** `@jeryu/web`. Output: `apps/web/dist/`.

---

## 1. Project structure

```
apps/web/
├── index.html
├── package.json                @jeryu/web
├── tsconfig.json
├── vite.config.ts
├── vitest.config.ts
├── playwright.config.ts
├── .eslintrc.cjs
├── .prettierrc.json
├── AGENTS.md
├── e2e/                        Playwright specs (W-T-09..18)
└── src/
    ├── main.tsx                React + StrictMode entry
    ├── global.d.ts
    ├── app/
    │   ├── App.tsx             top-level provider tree
    │   ├── providers.tsx       Query, Realtime, Theme, Keyboard
    │   └── router.tsx          React Router 6 route table
    ├── api/
    │   ├── client.ts           apiGet / apiSend / apiPatch / apiDelete
    │   ├── endpoints.ts        all /api/v1/* URL builders
    │   ├── types.ts            re-exports of contracts/generated/*
    │   └── websocket.ts        JeRyuWsClient (transport)
    ├── layout/
    │   ├── AppShell.tsx        global frame
    │   ├── CommandPalette.tsx  ⌘K
    │   ├── GlobalHeader.tsx
    │   ├── LeftNav.tsx
    │   ├── LiveActivityDock.tsx
    │   ├── RepoSwitcher.tsx
    │   └── StatusBar.tsx
    ├── pages/
    │   ├── DashboardPage.tsx
    │   ├── RepositoriesPage.tsx
    │   ├── RepositoryOverviewPage.tsx
    │   ├── RepositoryCodePage.tsx
    │   ├── RepositoryFilePage.tsx
    │   ├── RepositoryMergeRequestsPage.tsx
    │   ├── MergeRequestPage.tsx
    │   ├── MergeRoomPage.tsx
    │   ├── RepositorySettingsPage.tsx
    │   ├── AdminSettingsPage.tsx
    │   ├── NotificationsPage.tsx
    │   ├── AuditPage.tsx
    │   ├── IssuesPage.tsx
    │   ├── StubPage.tsx
    │   ├── NotFoundPage.tsx
    │   └── page.css
    ├── components/
    │   ├── KeyboardShortcutsOverlay.tsx
    │   ├── action/             Action preview + receipt widgets
    │   ├── browser/            Tree, blob, raw, markdown viewers
    │   ├── merge/              Passport, diff, threads
    │   ├── repo/               Summary, switcher entries
    │   └── state/              Loading / empty / error / forbidden surfaces
    ├── hooks/
    │   ├── useBootstrap.ts
    │   ├── useRealtime.ts
    │   ├── useRepositories.ts
    │   ├── useRepository.ts
    │   ├── useRepoTree.ts
    │   ├── useRefs.ts
    │   ├── useBlob.ts
    │   ├── useMarkdown.ts
    │   ├── useResolveRepo.ts
    │   └── useKeyboard.ts
    ├── stores/
    │   ├── realtimeStore.ts    Zustand binding for the WS transport
    │   ├── selectionStore.ts   focused entity per page
    │   ├── commandStore.ts     command palette open state
    │   └── preferencesStore.ts theme, density, shortcuts
    ├── styles/
    │   ├── tokens.css          design tokens (3 themes)
    │   └── app.css             global resets + utility primitives
    └── test/
        ├── mocks/              MSW handlers
        └── server.ts           Vitest setup with MSW
```

---

## 2. Design tokens

`src/styles/tokens.css` is the single source of truth for color,
typography, spacing, radius, shadow, motion, and z-index. Mission-
control aesthetic shared with the JeRyu TUI: dark first-class, light
polished daylight, high-contrast WCAG AAA-leaning.

Themes are applied via `[data-theme]` on `<html>`:

| Selector | Theme |
|---|---|
| `:root` | Light theme (default). |
| `@media (prefers-color-scheme: dark)` | Dark overrides (when no explicit `data-theme`). |
| `[data-theme="light"]` | Force light (overrides media query). |
| `[data-theme="dark"]` | Force dark. |
| `[data-theme="high-contrast"]` | High contrast. |

Token vocabulary (excerpt):

```
--color-bg-{0,1,2,3}                  page, surface, raised, inset
--color-fg-{primary,secondary,muted}  body, secondary, tertiary text
--color-accent-{primary,success,warning,danger,info}
--color-accent-{*}-bg                 tinted backgrounds
--color-border-{subtle,strong}
--font-{sans,mono}
--font-size-{xs,sm,base,md,lg,xl,2xl,3xl}
--space-{1..8}                        4 8 12 16 20 24 32 48
--radius-{sm,md,lg,full}
--shadow-{1,2,3}
--duration-{fast,std,slow}
--ease-{standard,emphasized}
--z-{base,dropdown,sticky,modal,popover,toast}
```

Only color, shadow, and border tokens vary per theme. Typography,
spacing, radii, motion, and z-index are theme-invariant.

---

## 3. API client

`src/api/client.ts` is the thin fetch wrapper used by every hook.

```ts
import { apiGet, apiSend, apiPatch, apiDelete, ApiError } from './client';

const repos = await apiGet<RepositoryListResponse>('/api/v1/repos');
const created = await apiSend<RepositorySummary>(
  '/api/v1/repos',
  body,
  { csrfToken, idempotencyKey: crypto.randomUUID() }
);
```

The wrapper:

- Sets `Accept: application/json` + `Content-Type: application/json` when
  a body is sent.
- Adds `Idempotency-Key` and `X-CSRF-Token` from `ApiRequestOptions` per
  WEB_WORK_CLAUDE.md §35.1.3.
- Sends `credentials: 'same-origin'` (cookies follow).
- Surfaces the structured error envelope as `ApiError` instances:

```ts
try {
  await apiSend(url, body, { csrfToken, idempotencyKey });
} catch (e) {
  if (e instanceof ApiError && e.code === 'merge_sha_stale') {
    // present recovery flow
  }
}
```

`ApiError` carries `status`, `code`, `details`, `requestId`, and
`eventCursor` (§35.1.11). Network failures surface as `ApiError` with
`status: 0` and `code: 'network_error'`.

---

## 4. WebSocket store

`src/stores/realtimeStore.ts` owns the singleton `JeRyuWsClient` and
exposes:

```ts
const status = useRealtimeStore((s) => s.status);   // 'idle' | 'connecting' | 'open' | 'closed' | 'reconnecting'
const events = useRealtimeStore((s) => s.events);   // rolling buffer (200)
const lastSeq = useRealtimeStore((s) => s.lastSeq); // bigint | null

useRealtimeStore.getState().subscribe(['repo.repo-uuid']);
useRealtimeStore.getState().connect();
```

Hooks consume the store:

- `useRealtime(scope)` mounts a subscription for the component lifetime
  and returns the rolling buffer for that scope.
- `useEventInvalidation((event) => …)` registers an invalidator that
  fires whenever any event arrives; used by feature hooks to invalidate
  React Query caches.

Gap detection:

- `lastSeq` is persisted to `sessionStorage` under `jeryu.ws.lastSeq.v1`
  so a page refresh resumes without a gap.
- On `snapshot_required` (server frame), the store invalidates the
  bootstrap query so React Query refetches the snapshot before
  re-subscribing.

See [`docs/WEBSOCKET_PROTOCOL.md`](../../docs/WEBSOCKET_PROTOCOL.md) for
the wire protocol.

---

## 5. Other stores

| Store | Responsibility |
|---|---|
| `selectionStore` | Focused entity per page (file, thread, blocker). Survives navigation within a route. |
| `commandStore` | ⌘K palette open state + filter buffer + command history. |
| `preferencesStore` | Theme, density, custom shortcut overrides; persisted to `localStorage` under `jeryu.prefs.v1`. |
| `realtimeStore` | (§4 above) |

All four are Zustand stores; React components select narrow slices with
the `useStore(s => …)` pattern to avoid re-render storms.

---

## 6. Keyboard shortcuts

The `useKeyboard.ts` foundation provides:

- `useKeyboardShortcut('Mod+K', handler, opts)` — registers a shortcut
  for the calling component's lifetime. `Mod` resolves to ⌘ on macOS,
  Ctrl elsewhere. Supports chord sequences (`g r`).
- `KeyboardProvider` — wraps the app and tracks all live shortcuts so
  the help overlay can render them.
- `KeyboardShortcutsOverlay` — the modal opened by `?`.

Built-in shortcuts:

| Shortcut | Where | Action |
|---|---|---|
| `⌘K` / `Ctrl+K` | global | Open the command palette. |
| `/` | global | Focus the global search input. |
| `?` | global | Show the keyboard shortcut overlay. |
| `Esc` | global | Close the topmost modal / palette / overlay. |
| `g r` | global | Go to Repositories. |
| `g m` | global | Go to Merge Room (own MRs needing attention). |
| `g s` | global | Go to Settings. |
| `g d` | global | Go to Dashboard. |
| `g n` | global | Go to Notifications. |
| `[` / `]` | merge cockpit | Previous / next file in the diff. |
| `j` / `k` | merge cockpit | Previous / next thread in the focused file. |
| `n` | merge cockpit | Jump to the next unresolved blocker. |
| `Enter` | composers | Submit. `Shift+Enter` inserts a newline. |

Add more by calling `useKeyboardShortcut` from a leaf component; the
overlay updates automatically.

---

## 7. Testing

Three pyramids run in CI:

| Layer | Tool | Where | Targets |
|---|---|---|---|
| Unit | Vitest + jsdom | `src/**/*.test.{ts,tsx}` | Pure helpers, hooks, store logic. |
| Component / integration | Vitest + Testing Library + MSW | `src/**/*.spec.tsx` | Components rendered with a mocked API. |
| E2E | Playwright | `e2e/*.spec.ts` | Full user workflows on a running BFF. |

Commands:

```bash
npm run test                # vitest run (unit + component)
npm run test:e2e            # playwright test
npx playwright install      # first-run only
npm run storybook           # http://127.0.0.1:6006
npm run build-storybook
```

E2E specs live under `apps/web/e2e/`. The harness expects a running
backend on `127.0.0.1:8787` with `JERYU_BACKEND_PROFILE=mock` (seeds 5
repos for offline determinism). `playwright.config.ts` sets the
`webServer` entry to launch both the BFF and Vite dev server when run
locally.

MSW mocks live in `src/test/mocks/` and are loaded by `src/test/server.ts`
which is registered via `vitest.config.ts` (`setupFiles`).

---

## 8. Accessibility

- **axe-core** runs in every Playwright spec via `@axe-core/playwright`;
  any `serious` or `critical` finding fails the run.
- **Storybook addon-a11y** runs the same rules per-story and surfaces
  warnings in the Storybook UI.
- Geometry checks (W-T-18) assert minimum 44×44 px hit targets for
  touch, no text overflow, no overlapping controls. Failures emit a
  screenshot tagged `geom-*.png` to `target/jankurai/ux-qa/`.
- Every interactive element has a visible focus ring driven by
  `--color-border-strong` + `outline-offset`.
- The activity dock is keyboard-navigable and announces new events via
  `aria-live="polite"`.

---

## 9. Bundle budget

| Limit | Value |
|---|---:|
| Initial shell JS (gzip) | **≤ 350 KB** |
| Route chunks per page | ≤ 80 KB gz |
| Largest single asset | ≤ 1 MB |
| First useful paint (local) | ≤ 1.5 s |
| Route transition (post-bootstrap) | ≤ 100 ms perceived |

Bundle size is checked in CI:

```bash
du -b dist/assets/index-*.js | awk '{print $1}' \\
  | xargs -I {} sh -c '[ {} -lt 358400 ] || echo "JS BUDGET EXCEEDED"'
```

Monaco editor (when wired) is **lazy-loaded** on the file view route; the
default shell never imports it. `shiki` is the lighter alternative
should the budget break.

---

## 10. Storybook

```bash
npm run storybook              # http://127.0.0.1:6006
npm run build-storybook        # output → storybook-static/
```

Stories cover:

- Every component under `src/components/`.
- Every layout primitive under `src/layout/`.
- Loading / empty / error / success / permission-denied states for each
  feature page (W-T-09 markers).

The Storybook composition is single-package; the `apps/ux-qa` workspace
ships its own marker checker (`ux-qa-check.mjs`) that asserts every
expected marker exists across Stories.

---

## 11. UX-QA proof receipts

`apps/web` produces:

- `target/jankurai/ux-qa/web-forge.<ts>.json` — per-page state coverage
  (loading, empty, error, success, permission-denied), screenshot
  references, axe results, geometry assertions.
- `target/jankurai/ux-qa.json` — aggregate top-level summary written by
  `jankurai ux audit --config agent/ux-qa.toml`.

Both files are consumed by the jankurai dashboard and the
`agent/ux-qa.toml` gate; receipts are kept outside the repo
(`target/jankurai/` is in `.gitignore`).

---

## 12. Reference

| Topic | Where |
|---|---|
| Architecture | [`docs/web-forge.md`](../../docs/web-forge.md) |
| REST surface | [`docs/WEB_API.md`](../../docs/WEB_API.md) |
| WebSocket protocol | [`docs/WEBSOCKET_PROTOCOL.md`](../../docs/WEBSOCKET_PROTOCOL.md) |
| Markdown rendering & XSS posture | [`docs/README_RENDERING.md`](../../docs/README_RENDERING.md) |
| Merge cockpit deep dive | [`docs/REVIEW_COCKPIT.md`](../../docs/REVIEW_COCKPIT.md) |
| Release milestones | [`ROADMAP.md`](../../ROADMAP.md) |
| Generated TS contracts | [`contracts/generated/`](../../contracts/generated/) |
| Generated OpenAPI | [`schemas/web-api.openapi.json`](../../schemas/web-api.openapi.json) |
| Generated WS schema | [`schemas/websocket-events.schema.json`](../../schemas/websocket-events.schema.json) |
| Source plan (authoritative) | [`WEB_WORK_CLAUDE.md`](../../WEB_WORK_CLAUDE.md) |
