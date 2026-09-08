// 25-action-matrix.spec.ts -- supplemental action tags for chrome, tools, and
// admin-denied controls that do not belong to a route-specific spec.

import { expect, test, type Page } from '@playwright/test';

import { mockBootstrap, mockRepoList } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

test('global chrome command palette, repo switcher, sidebar, notifications, not-found, and logout @action:chrome.command_palette @action:chrome.repo_switcher @action:chrome.sidebar_collapse @action:notifications.popover @action:chrome.not_found_recovery @action:auth.logout', async ({
  page,
}) => {
  await mockBootstrap(page, {
    login: '@e2e',
    display_name: 'E2E Tester',
  });
  await mockRepoList(page, [
    {
      id: { host: 'jeryu', owner: 'alice', name: 'jeryu' },
      default_branch: 'main',
      visibility: 'internal',
    },
  ]);
  let loggedOut = false;
  await page.route('**/api/v1/auth/me', async (route, request) => {
    if (request.method() !== 'GET') {
      await route.fallback();
      return;
    }
    if (loggedOut) {
      await route.fulfill({
        status: 401,
        contentType: 'application/json',
        body: JSON.stringify({
          error: { code: 'unauthorized', message: 'login required' },
        }),
      });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        login: '@e2e',
        role: 'user',
        mustChangePassword: false,
        csrfToken: 'e2e-csrf',
      }),
    });
  });
  await page.route('**/api/v1/auth/logout', async (route, request) => {
    if (request.method() !== 'POST') {
      await route.fallback();
      return;
    }
    loggedOut = true;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true }),
    });
  });

  await page.goto('/');
  await expect(page.locator('.app-shell')).toBeVisible({ timeout: 10_000 });

  await page.getByRole('button', { name: 'Open command palette' }).click();
  await page.getByRole('combobox', { name: 'Command palette' }).fill('Repositories');
  await page.getByText('Go to Repositories').click();
  await expect(page).toHaveURL(/\/repos$/);

  await page.getByRole('button', { name: 'Switch repository' }).click();
  await expect(
    page.getByRole('combobox', { name: 'Command palette' })
  ).toBeVisible();
  await page.keyboard.press('Escape');

  await page.getByRole('button', { name: 'Collapse sidebar' }).click();
  await expect(page.getByRole('button', { name: 'Expand sidebar' })).toBeVisible();
  await page.getByRole('button', { name: 'Expand sidebar' }).click();

  await page.getByRole('button', { name: 'Notifications (none unread)' }).click();
  const notifications = page.getByRole('dialog', { name: 'Notifications' });
  await expect(notifications).toBeVisible();
  await notifications.getByRole('link', { name: 'View all' }).click();
  await expect(page).toHaveURL(/\/notifications$/);

  await page.goto('/missing/action-matrix-route');
  await expect(page).toHaveURL(/\/missing\/action-matrix-route$/);
  await expect(page.getByRole('heading', { name: 'Page not found' })).toBeVisible({
    timeout: 10_000,
  });
  await page.getByRole('button', { name: 'Back to dashboard' }).click();
  await expect(page).toHaveURL(/\/repos\/family\/jeryu-split$/);

  await page.getByRole('button', { name: 'Log out' }).click();
  await expect(page.getByRole('heading', { name: 'Git for agents.' })).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.getByRole('button', { name: 'Log in' })).toBeVisible({
    timeout: 10_000,
  });
});

test('tools scan, expand, propose, ignore, tool fleet, and non-admin settings @action:tools.scan @action:tools.expand_cluster @action:tools.propose @action:tools.ignore @action:tool_fleet.render @action:admin.denied', async ({
  page,
}) => {
  await mockBootstrap(page, {
    login: '@viewer',
    auth: { role: 'user', csrfToken: 'csrf-viewer' },
  });
  await mockTooling(page);

  page.once('dialog', (dialog) => dialog.accept('covered by existing helper'));
  await page.goto('/tools');
  await expect(page.getByTestId('tools-page')).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId('rail-tool-action-coverage')).toBeVisible();
  await page.getByTestId('run-scan-button').click();
  await expect(page.getByTestId('scan-progress-panel')).toBeVisible();
  await page.getByRole('button', { name: /Shared API client/ }).click();
  const cluster = page.getByTestId('cluster-cluster-action');
  await expect(cluster).toBeVisible();
  await cluster.getByRole('button', { name: 'Propose tool' }).click();
  await expect(cluster).toContainText('proposal filed');
  await cluster.getByRole('button', { name: 'Ignore' }).click();

  await page.goto('/tool-fleet');
  await expect(page.getByTestId('tool-fleet-page')).toBeVisible();
  await expect(page.getByTestId('tool-row-action-coverage')).toContainText(
    'alice/jeryu'
  );

  await page.goto('/settings');
  await expect(page.getByTestId('settings-page')).toBeVisible();
  await expect(page.getByText('Users and repository access')).toHaveCount(0);
});

async function mockTooling(page: Page): Promise<void> {
  await page.route('**/api/v1/tools/registry/summary', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        generated_at: '2026-07-03T00:00:00Z',
        tool_count: 1,
        published_count: 1,
        building_count: 0,
        proposed_count: 0,
        deprecated_count: 0,
        adopting_repo_count: 1,
        candidate_repo_count: 1,
        open_task_count: 0,
        realized_loc_saved: 120,
        anticipated_loc_saved: 80,
        tools: [
          {
            id: 'action-coverage',
            name: 'Action Coverage',
            kind: 'ts-lib',
            status: 'published',
            adopting_repo_count: 1,
            candidate_repo_count: 1,
            loc_saved: 120,
            loc_saved_estimate: 80,
          },
        ],
      }),
    });
  });
  await page.route('**/api/v1/tool-finder/dashboard**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        generated_at: '2026-07-03T00:00:00Z',
        scan: {
          scanned_at: '1783036800000',
          scan_id: 'scan-1',
          repo_count: 1,
          file_count: 3,
        },
        family_count: 1,
        cluster_count: 1,
        candidate_loc_saved: 80,
        families: [
          {
            family_id: 'family-action',
            label: 'Shared API client',
            category: 'tool-candidate',
            language: 'typescript',
            anticipated_loc_saved: 80,
            occurrence_count: 3,
            file_count: 3,
            repos: ['alice/jeryu', 'jeryu/jeryu-web'],
            clusters: [
              {
                cluster_id: 'cluster-action',
                category: 'tool-candidate',
                score: 91,
                occurrence_count: 3,
                repo_count: 2,
                file_count: 3,
                total_lines: 90,
                language: 'typescript',
                insight: 'Shared API client wrapper',
                normalized_preview: 'function apiClient() {}',
                anticipated_loc_saved: 80,
                suggested_name: 'api-client',
                suggested_kind: 'ts-lib',
                ignored: false,
                occurrences: [
                  {
                    repo_id: 'alice/jeryu',
                    commit_sha: 'abcdef1234567890abcdef1234567890abcdef12',
                    path: 'src/api.ts',
                    start_line: 1,
                    end_line: 20,
                    language: 'typescript',
                    is_test: false,
                  },
                ],
              },
            ],
          },
        ],
      }),
    });
  });
  let scanStarted = false;
  await page.route('**/api/v1/tool-finder/scan', async (route, request) => {
    if (request.method() === 'POST') {
      scanStarted = true;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        scan_id: scanStarted ? 1 : 0,
        phase: scanStarted ? 'scan' : 'idle',
        running: scanStarted,
        repos_total: 1,
        repos_done: scanStarted ? 0 : 1,
        files_scanned: scanStarted ? 1 : 3,
        files_skipped: 0,
        clusters_found: 1,
        families_found: 1,
        current_repo: 'alice/jeryu',
        started_at: scanStarted ? '2026-07-03T00:00:00Z' : null,
        finished_at: null,
        error: null,
      }),
    });
  });
  await page.route('**/api/v1/tool-finder/propose/*', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        cluster_id: 'cluster-action',
        proposal_id: 'proposal-1',
        task_id: 'task-1',
        message: 'proposal filed',
      }),
    });
  });
  await page.route('**/api/v1/codegraph/tool-build/clusters/*/feedback', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true }),
    });
  });
  await page.route('**/api/v1/fleet/tool-adoption', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        repos_scored: 1,
        tools: [
          {
            tool: 'action-coverage',
            category: 'proof',
            adopting_repos: ['alice/jeryu'],
            applicable_missing_repos: ['jeryu/jeryu-web'],
          },
        ],
      }),
    });
  });
}
