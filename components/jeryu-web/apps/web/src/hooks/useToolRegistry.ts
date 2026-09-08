// useToolRegistry.ts — React Query hook for `GET /api/v1/tools/registry/summary`.
//
// Read-only snapshot of the reusable-tool registry owned by the special
// `jeryu-tool` repo (the family "tool control plane"). Powers the gold box at
// the top of the repositories grid. Mirrors `useToolFleet`/`useRepositories`:
// same query client, the shared `apiGet` error handling, and a 30 s stale
// window. Consumers degrade to nothing on loading/error/empty so the box never
// breaks the page when the backend handler is not yet deployed.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { ToolRegistrySummary } from '../api/types';

export function useToolRegistry(): UseQueryResult<ToolRegistrySummary, Error> {
  return useQuery({
    queryKey: ['tools', 'registry', 'summary'],
    queryFn: ({ signal }) =>
      apiGet<ToolRegistrySummary>(endpoints.toolRegistrySummary(), { signal }),
    staleTime: 30_000,
    // The endpoint may not exist yet (backend in parallel). A 404/network
    // miss is a permanent "not ready" signal for this surface, not a flake —
    // don't hammer it with retries; the box just renders null.
    retry: false,
  });
}
