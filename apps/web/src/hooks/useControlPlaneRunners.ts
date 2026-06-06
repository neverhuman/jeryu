// useControlPlaneRunners.ts - React Query wrapper around the runner-fabric snapshot.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { RunnerFabricResponse } from '../api/types';

export const CONTROL_PLANE_RUNNERS_QUERY_KEY: readonly [
  'control-plane',
  'runners',
] = ['control-plane', 'runners'];

export function useControlPlaneRunners(): UseQueryResult<
  RunnerFabricResponse,
  Error
> {
  return useQuery({
    queryKey: CONTROL_PLANE_RUNNERS_QUERY_KEY,
    queryFn: ({ signal }) =>
      apiGet<RunnerFabricResponse>(endpoints.controlPlaneRunners(), {
        signal,
      }),
    staleTime: 15_000,
    refetchOnWindowFocus: true,
  });
}
