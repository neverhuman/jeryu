// 23-auth-hardening.spec.ts — public portal auth/data-isolation browser proof.
//
// The BFF e2e harness usually runs with local-dev trust enabled, so this spec
// mocks the auth endpoints at the browser network boundary. That still drives
// the real AuthProvider, AuthPage, fetch client, AppShell gate, settings panel,
// and repository-family browser without depending on mutable backend accounts.

import { expect, test, type Page, type Route } from '@playwright/test';

import { mockBootstrap, mockRepoList } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

interface AuthUserWire {
  login: string;
  role: 'admin' | 'user';
  mustChangePassword: boolean;
  csrfToken?: string | null;
}

const splitRepos = [
  {
    id: { host: 'jeryu', owner: 'jeryu', name: 'jeryu-core' },
    family: 'jeryu-split',
    description: 'Granted core repository.',
  },
  {
    id: { host: 'jeryu', owner: 'jeryu', name: 'jeryu-web' },
    family: 'jeryu-split',
    description: 'Granted web repository.',
  },
];
const loginFormCredential = 'auth-form-input-0001';
const signupFormCredential = 'signup-form-input-0001';
const currentPasswordValue = 'initial-password-123';
const replacementPasswordValue = 'replacement-password-456';
const adminResetPasswordValue = 'initial-admin-reset-123';

test.describe('Auth hardening browser proof', () => {
  test('signed-out visitor logs in and sees only granted split repositories @action:auth.login @action:authz.granted_repos_only', async ({
    page,
  }) => {
    await mockBootstrap(page, { login: 'jordanh', auth: null });
    await mockAuthMe(page, null);
    await mockRepoList(page, splitRepos);

    const loginBodies: unknown[] = [];
    await page.route('**/api/v1/auth/login', async (route, request) => {
      loginBodies.push(request.postDataJSON());
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          login: 'jordanh',
          role: 'user',
          mustChangePassword: false,
          csrfToken: 'csrf-jordanh',
        }),
      });
    });

    await page.goto('/');
    await expect(page.getByRole('heading', { name: 'Git for agents.' })).toBeVisible({
      timeout: 10_000,
    });
    await expect(
      page.getByText(
        'Made by agents, for agents: a Rust forge core with a React cockpit'
      )
    ).toBeVisible();
    await expect(
      page.getByText(
        'Issue -> agent session -> evidence -> PR -> gated merge -> autonomous deploy'
      )
    ).toBeVisible();
    await page.getByRole('button', { name: 'Log in' }).click();
    await expect(page.getByRole('tab', { name: 'Login' })).toHaveAttribute(
      'aria-selected',
      'true'
    );

    await page.getByLabel('Username').fill('jordanh');
    await page.getByLabel('Password').fill(loginFormCredential);
    await page.getByLabel('Remember me').check();
    await page.getByRole('button', { name: 'Login' }).click();

    await expect(page).toHaveURL(/\/repos\/family\/jeryu-split/, {
      timeout: 10_000,
    });
    const browser = page.locator('section.split-browser');
    await expect(browser).toBeVisible({ timeout: 10_000 });
    await expect(browser.locator('.split-browser__repo')).toHaveCount(2);
    await expect(browser).toContainText('jeryu-core');
    await expect(browser).toContainText('jeryu-web');
    await expect(browser).not.toContainText('jeryu-deploy');
    expect(loginBodies).toEqual([
      { login: 'jordanh', password: loginFormCredential, rememberMe: true },
    ]);
  });

  test('new signup receives no repository visibility by default @action:auth.signup @action:authz.signup_no_repos', async ({ page }) => {
    await mockBootstrap(page, { login: 'jepsont', auth: null });
    await mockAuthMe(page, null);
    await mockRepoList(page, []);

    await page.route('**/api/v1/auth/signup', async (route, request) => {
      expect(request.postDataJSON()).toEqual({
        login: 'jepsont',
        password: signupFormCredential,
      });
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          login: 'jepsont',
          role: 'user',
          mustChangePassword: false,
          csrfToken: 'csrf-jepsont',
        }),
      });
    });

    await page.goto('/signup');
    await expect(page.locator('.boot__story')).toBeVisible({
      timeout: 10_000,
    });
    await expect(
      page.getByRole('heading', { name: 'Git for agents.' })
    ).toBeVisible();
    await expect(page.getByRole('tab', { name: 'Sign up' })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    await expect(page.getByLabel('Remember me')).toHaveCount(0);
    await page.getByLabel('Username').fill('jepsont');
    await page.getByLabel('Password').fill(signupFormCredential);
    await page.getByRole('button', { name: 'Create account' }).click();

    await expect(page).toHaveURL(/\/repos\/family\/jeryu-split/, {
      timeout: 10_000,
    });
    await expect(page.getByText('No repositories in this family')).toBeVisible();
    await expect(page.locator('section.split-browser')).toHaveCount(0);
    await expect(page.locator('.split-browser__repo')).toHaveCount(0);
  });

  test('authenticated visitors are redirected away from auth routes', async ({
    page,
  }) => {
    await mockBootstrap(page, { login: 'jordanh', auth: null });
    await mockAuthMe(page, {
      login: 'jordanh',
      role: 'user',
      mustChangePassword: false,
      csrfToken: 'csrf-jordanh',
    });
    await mockRepoList(page, splitRepos);

    await page.goto('/login');
    await expect(page).toHaveURL(/\/repos\/family\/jeryu-split/, {
      timeout: 10_000,
    });
    await expect(page.locator('section.split-browser')).toBeVisible({
      timeout: 10_000,
    });

    await page.goto('/signup');
    await expect(page).toHaveURL(/\/repos\/family\/jeryu-split/, {
      timeout: 10_000,
    });
    await expect(page.locator('section.split-browser')).toBeVisible({
      timeout: 10_000,
    });
  });

  test('forced password change sends CSRF @action:auth.force_password_change', async ({
    page,
  }) => {
    await mockBootstrap(page, { login: 'jordanh', auth: null });
    await mockAuthMe(page, {
      login: 'jordanh',
      role: 'user',
      mustChangePassword: true,
      csrfToken: 'csrf-temp',
    });
    await mockRepoList(page, splitRepos);

    let csrfHeader: string | null = null;
    await page.route('**/api/v1/auth/password', async (route, request) => {
      csrfHeader = request.headers()['x-jeryu-csrf'] ?? null;
      expect(request.postDataJSON()).toEqual({
        currentPassword: currentPasswordValue,
        newPassword: replacementPasswordValue,
      });
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          login: 'jordanh',
          role: 'user',
          mustChangePassword: false,
          csrfToken: 'csrf-after-change',
        }),
      });
    });

    await page.goto('/');
    await expect(page.getByRole('button', { name: 'Change password' })).toBeVisible({
      timeout: 10_000,
    });
    const passwordFields = page.locator('input[type="password"]');
    await expect(passwordFields).toHaveCount(2);
    await passwordFields.first().fill(currentPasswordValue);
    await page.getByLabel('New password').fill(replacementPasswordValue);
    await page.getByRole('button', { name: 'Change password' }).click();

    await expect(page.locator('section.split-browser')).toBeVisible({
      timeout: 10_000,
    });
    expect(csrfHeader).toBe('csrf-temp');
  });

  test('admin reset and repo grant use CSRF-protected mutations @action:admin.reset_password_success @action:admin.grant_repo_success', async ({
    page,
  }) => {
    await mockBootstrap(page, { login: 'jeryu-admin', auth: null });
    await mockAuthMe(page, {
      login: 'jeryu-admin',
      role: 'admin',
      mustChangePassword: false,
      csrfToken: 'csrf-admin',
    });
    await mockRepoList(page, []);

    const unsafeCsrfHeaders: string[] = [];
    await mockAdminUsers(page);
    await page.route(
      /\/api\/v1\/admin\/users\/[^/]+\/reset-password$/,
      async (route, request) => {
        unsafeCsrfHeaders.push(request.headers()['x-jeryu-csrf'] ?? '');
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            login: 'jordanh',
            password: adminResetPasswordValue,
          }),
        });
      }
    );
    await page.route(
      /\/api\/v1\/admin\/repos\/[^/]+\/[^/]+\/grants\/[^/]+$/,
      async (route, request) => {
        unsafeCsrfHeaders.push(request.headers()['x-jeryu-csrf'] ?? '');
        expect(request.postDataJSON()).toEqual({ access: 'read' });
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({ ok: true }),
        });
      }
    );

    await page.goto('/settings');
    await expect(page.getByText('Users and repository access')).toBeVisible({
      timeout: 10_000,
    });
    await page.getByRole('button', { name: 'Reset password' }).click();
    await expect(page.getByText(adminResetPasswordValue)).toBeVisible();

    const adminRegion = page.getByRole('region', {
      name: 'Users and repository access',
    });
    await adminRegion.getByRole('textbox', { name: 'Owner' }).fill('jeryu');
    await adminRegion.getByRole('textbox', { name: 'Repo' }).fill('jeryu-web');
    await adminRegion.getByRole('textbox', { name: 'User' }).fill('jordanh');
    await adminRegion.getByRole('radio', { name: 'read' }).click();
    await adminRegion.getByRole('button', { name: 'Grant access' }).click();
    await expect(page.getByText('Granted')).toBeVisible();
    expect(unsafeCsrfHeaders).toEqual(['csrf-admin', 'csrf-admin']);
  });
});

async function mockAuthMe(
  page: Page,
  user: AuthUserWire | null
): Promise<void> {
  await page.route('**/api/v1/auth/me', async (route: Route, request) => {
    if (request.method() !== 'GET') {
      await route.fallback();
      return;
    }
    if (user) {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(user),
      });
      return;
    }
    await route.fulfill({
      status: 401,
      contentType: 'application/json',
      body: JSON.stringify({
        error: { code: 'unauthorized', message: 'login required' },
      }),
    });
  });
}

async function mockAdminUsers(page: Page): Promise<void> {
  await page.route('**/api/v1/admin/users', async (route: Route, request) => {
    if (request.method() !== 'GET') {
      await route.continue();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        {
          login: 'jordanh',
          role: 'user',
          created_at: '2026-07-03T00:00:00Z',
          updated_at: '2026-07-03T00:00:00Z',
        },
      ]),
    });
  });
}
