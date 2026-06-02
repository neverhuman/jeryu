// useBootstrap.ts — React Query wrapper around `GET /api/v1/bootstrap`.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { WebBootstrap } from '../api/types';

export const BOOTSTRAP_QUERY_KEY: readonly ['bootstrap'] = ['bootstrap'];

export function useBootstrap(): UseQueryResult<WebBootstrap, Error> {
  return useQuery({
    queryKey: BOOTSTRAP_QUERY_KEY,
    queryFn: ({ signal }) =>
      apiGet<WebBootstrap>(endpoints.bootstrap(), { signal }),
    staleTime: 60_000,
    refetchOnWindowFocus: true,
  });
}
