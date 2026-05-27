// endpoints.ts — typed URL builders (W-FE-03).
//
// Single source of truth for every API path so URL bugs surface at
// typecheck time. All paths are versioned (§35.1.1) under `/api/v1/`.

export const endpoints = {
  bootstrap: (): string => '/api/v1/bootstrap',

  repos: (): string => '/api/v1/repos',
  repo: (id: string): string => `/api/v1/repos/${encodeURIComponent(id)}`,
  refs: (id: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/refs`,
  tree: (id: string, params: { ref: string; path?: string }): string => {
    const qs = new URLSearchParams({ ref: params.ref });
    if (params.path !== undefined && params.path !== '') {
      qs.set('path', params.path);
    }
    return `/api/v1/repos/${encodeURIComponent(id)}/tree?${qs.toString()}`;
  },
  blob: (
    id: string,
    params: { ref: string; path: string; render?: 'html' }
  ): string => {
    const qs = new URLSearchParams({ ref: params.ref, path: params.path });
    if (params.render) qs.set('render', params.render);
    return `/api/v1/repos/${encodeURIComponent(id)}/blob?${qs.toString()}`;
  },
  raw: (id: string, params: { ref: string; path: string }): string => {
    const qs = new URLSearchParams({ ref: params.ref, path: params.path });
    return `/api/v1/repos/${encodeURIComponent(id)}/raw?${qs.toString()}`;
  },
  readme: (id: string, ref?: string): string => {
    const base = `/api/v1/repos/${encodeURIComponent(id)}/readme`;
    return ref ? `${base}?ref=${encodeURIComponent(ref)}` : base;
  },
  compare: (id: string, base: string, head: string): string => {
    const qs = new URLSearchParams({ base, head });
    return `/api/v1/repos/${encodeURIComponent(id)}/compare?${qs.toString()}`;
  },
  mergeRequests: (id: string, state?: string): string => {
    const base = `/api/v1/repos/${encodeURIComponent(id)}/merge-requests`;
    return state ? `${base}?state=${encodeURIComponent(state)}` : base;
  },
  mergeRequest: (id: string, iid: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/merge-requests/${encodeURIComponent(iid)}`,
  mergeRequestDiff: (id: string, iid: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/merge-requests/${encodeURIComponent(iid)}/diff`,
  mergeRequestChecks: (id: string, iid: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/merge-requests/${encodeURIComponent(iid)}/checks`,
  mergeRequestThreads: (id: string, iid: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/merge-requests/${encodeURIComponent(iid)}/threads`,
  mergeRequestReviews: (id: string, iid: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/merge-requests/${encodeURIComponent(iid)}/reviews`,
  mergeRequestComments: (id: string, iid: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/merge-requests/${encodeURIComponent(iid)}/comments`,
  mergeRequestApprove: (id: string, iid: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/merge-requests/${encodeURIComponent(iid)}/approve`,
  mergeRequestMerge: (id: string, iid: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/merge-requests/${encodeURIComponent(iid)}/merge`,
  issues: (id: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/issues`,
  settings: (id: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/settings`,
  settingsPreview: (id: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/settings/preview`,

  ws: (): string => '/api/v1/ws',
  markdownRender: (): string => '/api/v1/markdown/render',
  search: (q: string): string =>
    `/api/v1/search?q=${encodeURIComponent(q)}`,
  activity: (): string => '/api/v1/activity',
} as const;

export type Endpoints = typeof endpoints;
