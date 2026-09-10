import { expect, test } from '@playwright/test';

test('browser authenticates against the standalone backend and retains its session @bff', async ({ page }, testInfo) => {
  const password = process.env.JERYU_BROWSER_PASSWORD;
  if (!password) throw new Error('disposable BFF password is required');
  await page.goto('/');
  await page.getByRole('button', { name: 'Log in', exact: true }).click();
  await page.getByLabel('Username', { exact: true }).fill('jeryu-admin');
  await page.getByLabel('Password', { exact: true }).fill(password);
  await page.getByRole('button', { name: 'Login', exact: true }).click();
  await expect(page).toHaveURL(/\/repos\/family\/jeryu-split/);
  await expect(page.getByRole('heading', { name: 'No repositories in this family' })).toBeVisible();
  await page.reload();
  await expect(page.getByRole('heading', { name: 'No repositories in this family' })).toBeVisible();
  const account = await page.evaluate(async () => {
    const response = await fetch('/api/v1/auth/me');
    if (!response.ok) throw new Error('persisted browser session was rejected');
    return response.json() as Promise<{ login: string; role: string }>;
  });
  expect(account.login).toBe('jeryu-admin');
  expect(account.role).toBe('admin');

  await page.getByRole('button', { name: 'Back to repositories' }).click();
  await page.getByRole('button', { name: /create repository/i }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('option', { name: 'local (unavailable)', exact: true })).toBeDisabled();
  await expect(dialog.getByRole('option', { name: 'internal (unavailable)', exact: true })).toBeDisabled();
  await expect(dialog.getByLabel('Topics (unavailable)', { exact: true })).toBeDisabled();
  await expect(dialog.getByLabel('Visibility', { exact: true })).toHaveValue('private');
  await testInfo.attach('standalone-repository-create-controls', {
    body: await page.screenshot({ fullPage: true }),
    contentType: 'image/png',
  });
  const name = `browser-${Date.now()}`;
  await dialog.getByLabel('Owner', { exact: true }).fill(account.login);
  await dialog.getByLabel('Name', { exact: true }).fill(name);
  await dialog.getByLabel('Initialize with README').check();
  await dialog.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(dialog.getByText('README.md', { exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect(page.getByText(name, { exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByText(name, { exact: true })).toBeVisible();
});
