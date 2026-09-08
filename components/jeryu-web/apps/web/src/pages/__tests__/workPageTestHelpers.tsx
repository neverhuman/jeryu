import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render } from '@testing-library/react';
import { MemoryRouter, Routes } from 'react-router-dom';
import { vi } from 'vitest';

export function renderRoute(route: string, child: JSX.Element): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[route]}>
        <Routes>{child}</Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

export interface CapturedRequest {
  method: string;
  path: string;
  body: unknown;
}

export interface MockRoute {
  method?: string;
  path: string;
  response: unknown | ((request: CapturedRequest) => unknown);
  status?: number;
}

export function mockFetch(
  routes: Array<[string, unknown] | MockRoute>
): CapturedRequest[] {
  const requests: CapturedRequest[] = [];
  const byPath = new Map<string, {
    response: MockRoute['response'];
    status: number;
  }>(
    routes.map((route) => {
      if (Array.isArray(route)) {
        return [`GET ${route[0]}`, { response: route[1], status: 200 }];
      }
      return [
        `${route.method ?? 'GET'} ${route.path}`,
        { response: route.response, status: route.status ?? 200 },
      ];
    })
  );
  vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
    const rawUrl = input instanceof Request ? input.url : String(input);
    const parsed = new URL(rawUrl, 'http://localhost');
    const path = parsed.pathname + parsed.search;
    const method =
      input instanceof Request
        ? init?.method ?? input.method
        : init?.method ?? 'GET';
    const body = parseRequestBody(init?.body);
    const request = { method, path, body };
    requests.push(request);
    const route = byPath.get(`${method} ${path}`);
    if (route === undefined) {
      return new Response(JSON.stringify({ error: { code: 'not_found', message: path } }), {
        status: 404,
        headers: { 'content-type': 'application/json' },
      });
    }
    const response =
      typeof route.response === 'function' ? route.response(request) : route.response;
    return new Response(JSON.stringify(response), {
      status: route.status,
      headers: { 'content-type': 'application/json' },
    });
  });
  return requests;
}

function parseRequestBody(body: BodyInit | null | undefined): unknown {
  if (typeof body !== 'string') return null;
  const parsed: unknown = JSON.parse(body);
  return parsed;
}

export function repoSummary(): Record<string, unknown> {
  return {
    id: { id: 'repo-1', host: 'jeryu', owner: 'alice', name: 'jeryu' },
    entity: { kind: 'repository', id: 'repo-1' },
    description: null,
    visibility: 'private',
    default_branch: 'main',
    family: null,
    repo_role: null,
    topics: [],
    language: null,
    health: 'healthy',
    open_pull_requests: 0,
    failing_checks: 0,
    running_jobs: 0,
    active_agents: 0,
    blocked_agents: 0,
    updated_at: '2026-07-02T00:00:00Z',
    clone_http_url: null,
    clone_ssh_url: null,
    available_actions: [],
  };
}

export function workItem(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: '7a733f01-5f6f-41b5-9f1c-b535a4b3d681',
    key: 'JRY-1',
    number: 1,
    repo: { id: 'repo-1', host: 'jeryu', owner: 'alice', name: 'jeryu' },
    title: 'Work item',
    body: null,
    status: 'ready',
    kind: 'task',
    priority: 'p2',
    labels: [],
    assignees: [],
    issue: null,
    pull_requests: [],
    created_at: '2026-07-02T00:00:00Z',
    updated_at: '2026-07-02T00:00:00Z',
    ...overrides,
  };
}
