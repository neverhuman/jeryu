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
  readmeUpdate: (id: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/readme`,
  compare: (id: string, base: string, head: string): string => {
    const qs = new URLSearchParams({ base, head });
    return `/api/v1/repos/${encodeURIComponent(id)}/compare?${qs.toString()}`;
  },
  pulls: (id: string, state?: string): string => {
    const base = `/api/v1/repos/${encodeURIComponent(id)}/pulls`;
    return state ? `${base}?state=${encodeURIComponent(state)}` : base;
  },
  pull: (id: string, prNumber: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/pulls/${encodeURIComponent(prNumber)}`,
  pullDiff: (id: string, prNumber: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/pulls/${encodeURIComponent(prNumber)}/diff`,
  pullChecks: (id: string, prNumber: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/pulls/${encodeURIComponent(prNumber)}/checks`,
  pullThreads: (id: string, prNumber: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/pulls/${encodeURIComponent(prNumber)}/threads`,
  pullReviews: (id: string, prNumber: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/pulls/${encodeURIComponent(prNumber)}/reviews`,
  pullComments: (id: string, prNumber: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/pulls/${encodeURIComponent(prNumber)}/comments`,
  pullApprove: (id: string, prNumber: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/pulls/${encodeURIComponent(prNumber)}/approve`,
  pullMerge: (id: string, prNumber: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/pulls/${encodeURIComponent(prNumber)}/merge`,
  issues: (id: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/issues`,
  settings: (id: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/settings`,
  settingsPreview: (id: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/settings/preview`,

  ws: (): string => '/api/v1/ws',
  markdownRender: (): string => '/api/v1/markdown/render',
  search: (
    q: string,
    options?: { kinds?: ReadonlyArray<string>; limit?: number }
  ): string => {
    const qs = new URLSearchParams({ q });
    if (options?.kinds && options.kinds.length > 0) {
      qs.set('kinds', options.kinds.join(','));
    }
    if (options?.limit !== undefined) {
      qs.set('limit', String(options.limit));
    }
    return `/api/v1/search?${qs.toString()}`;
  },
  activity: (): string => '/api/v1/activity',
  controlPlaneStatus: (): string => '/api/v1/control-plane/status',
  controlPlanePriorities: (limit?: number): string => {
    const base = '/api/v1/control-plane/priorities';
    return limit ? `${base}?limit=${encodeURIComponent(String(limit))}` : base;
  },
  controlPlaneRepoGraph: (params?: {
    repo?: string;
    clusterKind?: string;
    query?: string;
    limit?: number;
  }): string => {
    const qs = new URLSearchParams();
    if (params?.repo) qs.set('repo', params.repo);
    if (params?.clusterKind) qs.set('cluster_kind', params.clusterKind);
    if (params?.query) qs.set('query', params.query);
    if (params?.limit !== undefined) qs.set('limit', String(params.limit));
    const suffix = qs.toString();
    return suffix
      ? `/api/v1/control-plane/repo-graph?${suffix}`
      : '/api/v1/control-plane/repo-graph';
  },
  controlPlaneArtifactsLatest: (repo?: string): string =>
    repo
      ? `/api/v1/control-plane/artifacts/latest?repo=${encodeURIComponent(repo)}`
      : '/api/v1/control-plane/artifacts/latest',
  controlPlaneRunners: (): string => '/api/v1/control-plane/runners',
  ecosystem: (): string => '/api/v1/ecosystem',
  toolBuildClusters: (params?: {
    repo?: string;
    limit?: number;
    includeIgnored?: boolean;
  }): string => {
    const qs = new URLSearchParams();
    if (params?.repo) qs.set('repo', params.repo);
    if (params?.limit !== undefined) qs.set('limit', String(params.limit));
    if (params?.includeIgnored) qs.set('include_ignored', 'true');
    const suffix = qs.toString();
    return suffix
      ? `/api/v1/codegraph/tool-build/clusters?${suffix}`
      : '/api/v1/codegraph/tool-build/clusters';
  },
  agentRuns: (): string => '/api/v1/agent-runs',
  repoAgentRuns: (id: string): string => `/api/v1/repos/${encodeURIComponent(id)}/agent-runs`,
  repoSessions: (id: string): string =>
    `/api/v1/repos/${encodeURIComponent(id)}/sessions`,
} as const;

export type Endpoints = typeof endpoints;
