// useToolFleet.ts — React Query hook for `GET /fleet/tool-adoption`.
//
// Projects every repo's latest recorded jankurai score (tool_adoption.items)
// into a per-tool adoption matrix. Read-only; powers the Tool Fleet page.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { ToolFleetResponse } from '../api/types';

export function useToolFleet(): UseQueryResult<ToolFleetResponse, Error> {
  return useQuery({
    queryKey: ['fleet', 'tool-adoption'],
    queryFn: ({ signal }) =>
      apiGet<ToolFleetResponse>(endpoints.fleetToolAdoption(), { signal }),
    staleTime: 30_000,
  });
}
