// mocks.ts — Playwright route-mocking helpers (W-T-12..17).
// Generated API mocks stay aligned with generated contract types here and
// with the MSW mock service worker component-test fixtures in src/test/.
//
// These helpers wrap `page.route(...)` so individual specs can mock out
// `/api/v1/*` JSON endpoints without sharing fragile cross-test state. The
// fixtures here intentionally do NOT depend on the real backend — Phase 3
// services may still be partially live. When a test wants the SPA to
// exercise the BFF and tolerate 502/404, omit the mock and let the live
// route through.
//
// Convention:
//   * Each helper takes the `page` plus the JSON payload to serve.
//   * Routes are registered with `page.route(...)` so they are scoped to a
//     single test's `BrowserContext`.
//   * Mocks return ApiError envelopes for non-2xx codes so the SPA's
//     ErrorState pulls a meaningful `code` / `message`.

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import type { Page, Route } from '@playwright/test';
import type { RunnerFabricResponse } from '../../src/api/types';

// Playwright 1.60's bundled TS compilation treats fixture files as ESM, so
// `__dirname` is unavailable. Resolve the JSON fixture relative to the
// helper file via `import.meta.url`.
const FIXTURES_DIR = path.dirname(fileURLToPath(import.meta.url));
const BOOTSTRAP_FIXTURE_PATH = path.resolve(
  FIXTURES_DIR,
  'data',
  'bootstrap.json'
);
const bootstrapJson = JSON.parse(
  readFileSync(BOOTSTRAP_FIXTURE_PATH, 'utf8')
) as Record<string, unknown> & {
  viewer: {
    login: string;
    display_name: string | null;
    global_permissions: string[];
    [key: string]: unknown;
  };
};

export interface ViewerOverride {
  /** Replace the bootstrap viewer.login. */
  login?: string;
  /** Replace the bootstrap viewer.display_name. */
  display_name?: string;
  /** Replace the entire viewer.global_permissions array. */
  global_permissions?: string[];
}

/**
 * Mock `GET /api/v1/bootstrap` so the SPA boots in a fully deterministic
 * state regardless of whether the backend is live. The default body is the
 * canonical Phase 2 fixture (`fixtures/data/bootstrap.json`); pass
 * `viewer` to override individual fields.
 */
export async function mockBootstrap(
  page: Page,
  viewer: ViewerOverride = {}
): Promise<void> {
  await page.route('**/api/v1/bootstrap', async (route: Route) => {
    const body = JSON.parse(JSON.stringify(bootstrapJson));
    if (viewer.login !== undefined) body.viewer.login = viewer.login;
    if (viewer.display_name !== undefined) {
      body.viewer.display_name = viewer.display_name;
    }
    if (viewer.global_permissions !== undefined) {
      body.viewer.global_permissions = viewer.global_permissions;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(body),
    });
  });
}

/** A `PoolRollup`-shaped object as it serializes onto the bootstrap `tui`. */
export interface MockPoolRollup {
  pool: string;
  tags?: string[];
  trust_tier?: string;
  paused?: boolean;
  queued_jobs?: number;
  running_jobs?: number;
  failed_jobs?: number;
  active_slots?: number;
  configured_max_slots?: number;
  online_runners?: number;
  stuck_runners?: number;
}

/**
 * Mock `GET /api/v1/bootstrap` with a populated `tui.pool_activity` +
 * `tui.system` snapshot so the /fleet page paints pool cards + the
 * system-health strip on first paint (the WS spine layers live deltas on top,
 * but Playwright cannot inject raw WS frames — see 08-ws-reconnect.spec.ts —
 * so the bootstrap snapshot is the deterministic e2e surface for /fleet).
 */
export async function mockFleetBootstrap(
  page: Page,
  pools: MockPoolRollup[]
): Promise<void> {
  await page.route('**/api/v1/bootstrap', async (route: Route) => {
    const body = JSON.parse(JSON.stringify(bootstrapJson)) as Record<
      string,
      unknown
    > & { tui?: Record<string, unknown> };
    const normalizedPools = pools.map((p) => ({
      pool: p.pool,
      tags: p.tags ?? [],
      trust_tier: p.trust_tier ?? 'trusted',
      paused: p.paused ?? false,
      queued_jobs: p.queued_jobs ?? 0,
      running_jobs: p.running_jobs ?? 0,
      failed_jobs: p.failed_jobs ?? 0,
      active_slots: p.active_slots ?? 0,
      configured_max_slots: p.configured_max_slots ?? p.active_slots ?? 0,
      online_runners: p.online_runners ?? 0,
      stuck_runners: p.stuck_runners ?? 0,
    }));
    body.tui = {
      generated_at: new Date().toISOString(),
      pool_activity: {
        repos: [{ repo: 'veox/redline', pools: pools.map((p) => p.pool) }],
        pools: normalizedPools,
        unplaceable: [],
        freshness: null,
      },
      system: {
        scm: { name: 'scm', status: 'healthy', latency_ms: 8, detail: null },
        database: {
          name: 'database',
          status: 'healthy',
          latency_ms: 2,
          detail: null,
        },
        sandbox: {
          name: 'sandbox',
          status: 'degraded',
          latency_ms: null,
          detail: 'slow',
        },
        cache: { name: 'cache', status: 'healthy', latency_ms: 1, detail: null },
        vault: { name: 'vault', status: 'warning', latency_ms: null, detail: null },
        runners: { online: 4, busy: 1, idle: 3, degraded: 0 },
      },
    };
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(body),
    });
  });
}

export async function mockControlPlaneRunners(
  page: Page,
  response: RunnerFabricResponse
): Promise<void> {
  await page.route('**/api/v1/control-plane/runners', async (route: Route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(response),
    });
  });
}

export interface MockRepoSummary {
  id: { host: string; owner: string; name: string };
  default_branch?: string;
  description?: string | null;
  visibility?: 'public' | 'internal' | 'private';
  topics?: string[];
  open_pull_requests?: number;
  failing_checks?: number;
  active_agents?: number;
}

/**
 * Mock `GET /api/v1/repos` with a list of repositories. The shape mirrors
 * the contract in `RepositoryListResponse` (`generated_at` / `total` /
 * `repositories` / `facets`) so the SPA picks them up without an envelope
 * translation layer. Only the base `/api/v1/repos` (with or without a
 * query string) is intercepted — sub-paths (`/repos/{id}/...`) must fall
 * through to per-resource mocks or the live BFF.
 */
export async function mockRepoList(
  page: Page,
  repos: MockRepoSummary[]
): Promise<void> {
  const repositories = repos.map((r) => normalizeRepo(r));
  const hosts = Array.from(new Set(repos.map((r) => r.id.host)));
  const families: string[] = [];
  await page.route('**/api/v1/repos**', async (route: Route, request) => {
    if (request.method() !== 'GET') {
      await route.continue();
      return;
    }
    const url = new URL(request.url());
    // Only the base /api/v1/repos collection — bail out for sub-resources
    // like /api/v1/repos/{id}, /api/v1/repos/{id}/tree, etc.
    const segments = url.pathname.split('/').filter(Boolean);
    if (segments.length !== 3) {
      await route.continue();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        generated_at: '2026-05-26T00:00:00Z',
        total: repositories.length,
        repositories,
        facets: {
          hosts,
          owners: Array.from(new Set(repos.map((r) => r.id.owner))),
          families,
          languages: [],
        },
      }),
    });
  });
}

/**
 * Mock `GET /api/v1/repos/{id}` so the SPA's `useResolveRepo` returns a
 * fully populated `RepositorySummary` without touching the live forge.
 */
export async function mockRepoLookup(
  page: Page,
  repo: MockRepoSummary
): Promise<void> {
  const summary = normalizeRepo(repo);
  await page.route('**/api/v1/repos/*', async (route: Route, request) => {
    if (request.method() !== 'GET') {
      await route.continue();
      return;
    }
    // Sub-paths like /repos/{id}/refs must not be swallowed here.
    const url = new URL(request.url());
    if (url.pathname.split('/').length > 4) {
      await route.continue();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ id: summary.id, summary }),
    });
  });
}

export interface MockPullRequest {
  number: string;
  title: string;
  state: 'open' | 'merged' | 'closed';
  head_sha: string;
  base_sha?: string;
  author?: { login: string; display_name?: string | null };
  approvals?: number;
  approvals_required?: number;
}

/**
 * Mock `GET /api/v1/repos/{id}/pulls/{number}`. The Phase-3 PR
 * cockpit consumes the `PullRequestDetail` shape; we serve the minimum
 * surface and add extension fields the SPA's selectors look at.
 */
export async function mockPullRequest(
  page: Page,
  pr: MockPullRequest
): Promise<void> {
  const body = {
    summary: {
      number: pr.number,
      title: pr.title,
      state: pr.state,
      head_sha: pr.head_sha,
      base_sha: pr.base_sha ?? 'base000000000000000000000000000000000000',
      author: pr.author ?? {
        login: '@author',
        display_name: 'PR Author',
      },
      approvals: pr.approvals ?? 0,
      approvals_required: pr.approvals_required ?? 1,
      created_at: '2026-05-26T00:00:00Z',
      updated_at: '2026-05-26T00:00:00Z',
    },
    threads: [],
    review_verdicts: [],
    passport: {
      status: pr.state === 'merged' ? 'merged' : 'open',
      blockers: [],
    },
  };
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/pulls\/[^/]+$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(body),
      });
    }
  );
}

export interface MockPullRequestDetail {
  repoId: string;
  number: string;
  title?: string;
  state?: 'open' | 'merged' | 'closed';
  head_sha: string;
  base_sha?: string;
  head_ref?: string;
  base_ref?: string;
  /** Passport verdict — controls whether the Merge button renders. */
  passport?: 'pass' | 'blocked';
  blockers?: Array<{ code: string; message: string; details?: string | null }>;
  /** ReviewPosture overrides. */
  approvals?: number;
  required_approvals?: number;
  unresolved_threads?: number;
  /** When true, `mergeable.can_merge` is set so the merge CTA enables. */
  can_merge?: boolean;
  passport_hash?: string | null;
}

/**
 * Mock `GET /api/v1/repos/{id}/pulls/{number}` with the *full*
 * `PullRequestDetail` wire shape (`contracts/generated/PullRequestDetail`) so
 * the real `PullRequestPage` cockpit hydrates and paints the live Review
 * sidebar — including the "Approve exact SHA <sha>" button. Unlike the
 * thinner `mockPullRequest`, this fixture mirrors every field the SPA's
 * selectors read (`summary.review`, `summary.mergeable`, `merge_passport`,
 * `passport_hash`) so specs can drive *real clicks* instead of
 * `page.evaluate(fetch)`.
 */
export async function mockPullRequestDetail(
  page: Page,
  pr: MockPullRequestDetail
): Promise<void> {
  const status = pr.passport ?? 'blocked';
  const canMerge = pr.can_merge ?? status === 'pass';
  const detail = {
    summary: {
      repo: { id: pr.repoId, host: 'jeryu', owner: 'neverhuman', name: 'jeryu' },
      number: Number(pr.number),
      entity: { kind: 'pull_request', id: `${pr.repoId}#${pr.number}` },
      title: pr.title ?? `PR #${pr.number}`,
      author: '@author',
      head_ref: pr.head_ref ?? 'feature/x',
      base_ref: pr.base_ref ?? 'main',
      head_sha: pr.head_sha,
      base_sha: pr.base_sha ?? 'base000000000000000000000000000000000000',
      state: pr.state ?? 'open',
      draft: false,
      mergeable: {
        level: canMerge ? 'mergeable' : 'blocked',
        can_merge: canMerge,
        reason: canMerge ? null : 'Passport blocked',
        exact_head_sha: pr.head_sha,
        required_gate: canMerge ? null : 'passport',
      },
      review: {
        required_approvals: pr.required_approvals ?? 1,
        approvals: pr.approvals ?? 0,
        changes_requested: 0,
        unresolved_threads: pr.unresolved_threads ?? 0,
        user_review_state: null,
      },
      checks: { total: 2, passing: 2, failing: 0, pending: 0, skipped: 0 },
      agents: {
        active_sessions: 0,
        proposed_patches: 0,
        evidence_packets: 0,
        blockers: 0,
      },
      labels: [],
      updated_at: '2026-05-26T00:00:00Z',
      passport_hash: pr.passport_hash ?? 'passport-hash-0001',
      available_actions: [
        { action_id: 'pull.approve', label: 'Approve', risk: null },
        { action_id: 'pull.merge', label: 'Merge', risk: 'medium' },
      ],
    },
    description: pr.title ?? null,
    merge_passport: {
      status,
      head_sha: pr.head_sha,
      blockers:
        status === 'blocked'
          ? (pr.blockers ?? [
              {
                code: 'passport_blocked_approvals',
                message: 'Required approver count not satisfied.',
                details: null,
              },
            ]).map((b) => ({ ...b, details: b.details ?? null }))
          : [],
      evaluated_at: '2026-05-26T00:00:00Z',
    },
    passport_hash: pr.passport_hash ?? 'passport-hash-0001',
  };
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/pulls\/[^/]+$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(detail),
      });
    }
  );
}

/**
 * Mock the PR list endpoint with a single PR so list-driven UIs can hydrate.
 */
export async function mockPullRequestList(
  page: Page,
  prs: MockPullRequest[]
): Promise<void> {
  const items = prs.map((pr) => ({
    repo: {
      id: 'jeryu:neverhuman/jeryu',
      host: 'jeryu',
      owner: 'neverhuman',
      name: 'jeryu',
    },
    number: Number(pr.number),
    entity: {
      kind: 'pull_request',
      id: `jeryu:neverhuman/jeryu#${pr.number}`,
    },
    title: pr.title,
    state: pr.state,
    draft: false,
    head_ref: 'feature/x',
    head_sha: pr.head_sha,
    base_ref: 'main',
    base_sha: pr.base_sha ?? 'base000000000000000000000000000000000000',
    author: pr.author?.login ?? '@author',
    mergeable: {
      level: 'blocked',
      can_merge: false,
      reason: 'fixture',
      exact_head_sha: pr.head_sha,
      required_gate: 'merge_passport',
    },
    review: {
      required_approvals: pr.approvals_required ?? 1,
      approvals: pr.approvals ?? 0,
      changes_requested: 0,
      unresolved_threads: 0,
      user_review_state: null,
    },
    checks: { total: 0, passing: 0, failing: 0, pending: 0, skipped: 0 },
    agents: {
      active_sessions: 0,
      proposed_patches: 0,
      evidence_packets: 0,
      blockers: 0,
    },
    labels: [],
    updated_at: '2026-05-26T00:00:00Z',
    passport_hash: null,
    available_actions: [],
  }));
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/pulls(\?.*)?$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ items, total: items.length }),
      });
    }
  );
}

/**
 * Force `POST /api/v1/repos/{id}/pulls/{number}/approve` to return
 * the `merge_sha_stale` error envelope so specs can drive the "your view is
 * out of date" UI branch. The server includes both `expected_sha` (what the
 * client sent) and `current_sha` (latest head); we mirror those keys.
 */
export async function forceDriftSha(
  page: Page,
  oldSha: string,
  newSha: string
): Promise<void> {
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/pulls\/[^/]+\/approve$/,
    async (route: Route, request) => {
      if (request.method() !== 'POST') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 409,
        contentType: 'application/json',
        body: JSON.stringify({
          error: {
            code: 'merge_sha_stale',
            message: 'Head SHA changed since you loaded this PR.',
            details: {
              expected_sha: oldSha,
              current_sha: newSha,
            },
            request_id: 'mock-sha-drift',
          },
        }),
      });
    }
  );
}

/**
 * Mock `GET /api/v1/repos/{id}/refs` so the BranchSelector + code browser
 * resolve `default_branch` without hitting the live forge.
 */
export async function mockRefs(
  page: Page,
  refs: Array<{ name: string; kind?: 'branch' | 'tag'; default?: boolean }> = []
): Promise<void> {
  const items = refs.length
    ? refs
    : [
        { name: 'main', kind: 'branch' as const, default: true },
        { name: 'develop', kind: 'branch' as const, default: false },
      ];
  const body = {
    items: items.map((r) => ({
      name: r.name,
      kind: r.kind ?? 'branch',
      target: '0'.repeat(40),
      is_default: r.default ?? false,
    })),
  };
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/refs(\?.*)?$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(body),
      });
    }
  );
}

/**
 * Mock `GET /api/v1/repos/{id}/tree` with a small file-tree payload.
 */
export async function mockTree(
  page: Page,
  entries: Array<{ path: string; kind: 'file' | 'dir' }> = []
): Promise<void> {
  const items = entries.length
    ? entries
    : [
        { path: 'README.md', kind: 'file' as const },
        { path: 'src', kind: 'dir' as const },
        { path: 'package.json', kind: 'file' as const },
      ];
  const body = items.map((entry) => ({
    path: entry.path,
    name: entry.path.split('/').pop() ?? entry.path,
    kind: entry.kind,
    size: entry.kind === 'file' ? 1024 : null,
    sha: '0'.repeat(40),
  }));
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/tree(\?.*)?$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(body),
      });
    }
  );
}

export interface MockRenderedReadme {
  html: string;
  toc?: Array<{ depth: number; id: string; text: string }>;
  links?: Array<Record<string, unknown>>;
}

/**
 * Mock `GET /api/v1/repos/{id}/readme` with a `RenderedMarkdown`-shaped
 * payload. Used by the README rendering smoke (W-T-11) to feed a fixed HTML
 * blob into the ReadmePanel so the test can assert sanitization invariants
 * without depending on a live forge repo.
 */
export async function mockReadme(
  page: Page,
  rendered: MockRenderedReadme
): Promise<void> {
  const body = {
    html: rendered.html,
    toc: rendered.toc ?? [],
    links: rendered.links ?? [],
    renderer_version: 'jeryu-md-renderer.v1',
    sanitizer_version: 'jeryu-md-sanitizer.v1',
    rendered_at: '2026-05-26T00:00:00Z',
  };
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/readme(\?.*)?$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(body),
      });
    }
  );
}

/**
 * Mock `GET /api/v1/repos/{id}/settings` with a minimal RepositorySettings
 * envelope so the settings page can render values.
 */
export async function mockSettings(page: Page, overrides: Record<string, unknown> = {}): Promise<void> {
  const settings = {
    general: { description: 'mocked', homepage_url: null },
    features: { issues: true, pull_requests: true, wiki: false, projects: false },
    access: { default_role: 'reporter' },
    branch_protection: [],
    merge: { strategy: 'merge_commit', squash: false },
    security: { signed_commits_required: false, secrets_scanning: true },
    notifications: { default_recipient: null },
    retention: { artifact_days: 30 },
    ci: { enabled: true, runner_pool: 'default' },
    agents: { enabled: false },
    ...overrides,
  };
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/settings(\?.*)?$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(settings),
      });
    }
  );
}

/**
 * Force `GET /api/v1/repos/{id}/settings` to return a `permission_denied`
 * envelope (403). The settings studio propagates this through `ApiError` and
 * renders the real `<PermissionDeniedState>` surface (role="alert") — letting
 * specs assert the perm-denied UI via navigation alone, with no synthetic
 * fetch. `missing` defaults to `settings.read` (the read gate the page checks).
 */
export async function forceSettingsForbidden(
  page: Page,
  missing = 'settings.read'
): Promise<void> {
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/settings(\?.*)?$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 403,
        contentType: 'application/json',
        body: JSON.stringify({
          error: {
            code: 'permission_denied',
            message: 'You need settings.read to view this repository.',
            details: { missing },
            request_id: 'mock-settings-forbidden',
          },
        }),
      });
    }
  );
}

/**
 * Mock `POST /api/v1/repos/{id}/settings/preview` so the SPA can render the
 * diff card without hitting the live forge. Returns a fixed receipt + warnings list.
 */
export async function mockSettingsPreview(
  page: Page,
  warnings: string[] = []
): Promise<void> {
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/settings\/preview$/,
    async (route: Route, request) => {
      if (request.method() !== 'POST') {
        await route.continue();
        return;
      }
      const body = JSON.parse(request.postData() ?? '{}') as Record<string, unknown>;
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          patch: body,
          warnings,
          requires_confirmation: warnings.length > 0,
          dry_run: true,
        }),
      });
    }
  );
}

export interface MockAgentRun {
  run_id: string;
  branch: string;
  runner: string;
  status: string;
  tty_live?: boolean;
  agent?: string;
  workcell_id?: string;
}

/**
 * Mock `GET /api/v1/repos/{id}/agent-runs` — the active-agents list backing
 * `RepositoryAgentsPage`. NOTE (per the task brief): this backend route is a
 * separate workstream and may not exist live yet, so the e2e suite mocks it
 * here. Matches the `{ items: RepoAgentSummary[] }` wire shape.
 */
export async function mockRepoAgentRuns(
  page: Page,
  runs: MockAgentRun[]
): Promise<void> {
  const items = runs.map((r) => ({
    run_id: r.run_id,
    branch: r.branch,
    runner: r.runner,
    status: r.status,
    tty_live: r.tty_live ?? false,
    agent: r.agent ?? null,
    shell_run_id: r.shell_run_id ?? null,
    workcell_id: r.workcell_id ?? null,
    updated_at: '2026-05-26T00:00:00Z',
  }));
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/agent-runs(\?.*)?$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ items }),
      });
    }
  );
}

export interface MockCreatedSession {
  run_id: string;
  branch?: string;
}

/**
 * Mock `POST /api/v1/repos/{id}/sessions` — the "New Session" creation route
 * backing the Agents lens button (a separate backend workstream). Returns the
 * `{ run_id, branch, base_oid, ws_scope, tty_topic, control_url, status_url }`
 * wire shape the SPA deep-links to and mounts the live terminal on.
 */
export async function mockCreateSession(
  page: Page,
  session: MockCreatedSession
): Promise<void> {
  const runId = session.run_id;
  const branch = session.branch ?? `agent/${runId}`;
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/sessions(\?.*)?$/,
    async (route: Route, request) => {
      if (request.method() !== 'POST') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 201,
        contentType: 'application/json',
        body: JSON.stringify({
          run_id: runId,
          branch,
          base_oid: 'b'.repeat(40),
          ws_scope: `agent_run.${runId}`,
          tty_topic: `agent_run.${runId}.tty`,
          control_url: `/api/v1/agent-runs/${runId}/control`,
          status_url: `/api/v1/agent-runs/${runId}/status`,
          shell_run_id: `shell-${runId}`,
        }),
      });
    }
  );
}

function normalizeRepo(repo: MockRepoSummary): Record<string, unknown> {
  // Per §35.1.2 the canonical `RepositoryId.id` is the opaque UUID-shaped
  // key used in `/api/v1/repos/{id}/...` sub-paths. The SPA's
  // `useResolveRepo` reads `summary.id.id` and feeds it into `endpoints.*`,
  // so the fixture must supply a stable string here. We use a
  // deterministic host:owner/name composite so the same input produces the
  // same URL across test runs (handy for cross-mock regex matching).
  const stableId = `${repo.id.host}:${repo.id.owner}/${repo.id.name}`;
  return {
    id: {
      id: stableId,
      host: repo.id.host,
      owner: repo.id.owner,
      name: repo.id.name,
    },
    entity: {
      kind: 'repository',
      id: stableId,
    },
    description: repo.description ?? null,
    visibility: repo.visibility ?? 'private',
    default_branch: repo.default_branch ?? 'main',
    family: null,
    topics: repo.topics ?? [],
    language: null,
    health: 'green',
    open_pull_requests: repo.open_pull_requests ?? 0,
    failing_checks: repo.failing_checks ?? 0,
    running_jobs: 0,
    active_agents: repo.active_agents ?? 0,
    blocked_agents: 0,
    updated_at: '2026-05-26T00:00:00Z',
    clone_http_url: `https://example.com/${repo.id.owner}/${repo.id.name}.git`,
    clone_ssh_url: `git@example.com:${repo.id.owner}/${repo.id.name}.git`,
    available_actions: [],
  };
}

/**
 * Mock the SSE TTY stream endpoint. Returns a minimal stream so the terminal
 * component initializes without errors.
 */
export async function mockTtyStream(
  page: Page,
  events?: Array<{ seq: number; stream: string; text: string }>,
): Promise<void> {
  await page.route("**/api/v1/agent-runs/*/tty/stream*", async (route: Route) => {
    const items = events ?? [
      { seq: 1, stream: "stdout", text: "$ ready\r\n" },
    ];
    const body = items
      .map(
        (evt) =>
          `data: ${JSON.stringify({
            seq: evt.seq,
            stream: evt.stream,
            text: evt.text,
            bytes_b64: null,
            exit_code: null,
          })}\n\n`,
      )
      .join("");
    await route.fulfill({
      status: 200,
      contentType: "text/event-stream",
      headers: { "Cache-Control": "no-cache" },
      body,
    });
  });
}

/**
 * Mock the REST control endpoint. Captures posted commands into the returned
 * array for assertion.
 */
export async function mockAgentControl(
  page: Page,
): Promise<Array<Record<string, unknown>>> {
  const controls: Array<Record<string, unknown>> = [];
  await page.route("**/api/v1/agent-runs/*/control", async (route: Route, request) => {
    if (request.method() !== "POST") {
      await route.continue();
      return;
    }
    try {
      const body = JSON.parse(request.postData() ?? "{}");
      controls.push(body);
    } catch {
      // ignore
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ accepted: true, control_seq: controls.length }),
    });
  });
  return controls;
}

/**
 * Mock the companion shell endpoint `POST /api/v1/agent-runs/:id/shell`.
 * Returns a stable `shell_run_id` so the split terminal test can assert
 * that both panes are mounted.
 */
export async function mockCompanionShell(
  page: Page,
  shellRunId = 'shell-001',
): Promise<void> {
  await page.route('**/api/v1/agent-runs/*/shell', async (route: Route, request) => {
    if (request.method() !== 'POST') {
      await route.continue();
      return;
    }
    await route.fulfill({
      status: 201,
      contentType: 'application/json',
      body: JSON.stringify({
        shell_run_id: shellRunId,
        status_url: `/api/v1/agent-runs/${shellRunId}`,
        tty_stream_url: `/api/v1/agent-runs/${shellRunId}/tty/stream`,
        control_url: `/api/v1/agent-runs/${shellRunId}/control`,
      }),
    });
  });
}
