import { expect, type APIRequestContext } from '@playwright/test';

/** Provision a session against the disposable standalone server. */
export async function loginBff(request: APIRequestContext): Promise<string> {
  const password = process.env.JERYU_BROWSER_PASSWORD;
  if (!password) throw new Error('BFF fixture password is required');
  const response = await request.post('/api/v1/auth/login', {
    data: { login: 'jeryu-admin', password },
  });
  expect(response.status(), 'disposable administrator login').toBe(200);
  const body = await response.json() as { csrfToken: string };
  expect(typeof body.csrfToken).toBe('string');
  return body.csrfToken;
}
