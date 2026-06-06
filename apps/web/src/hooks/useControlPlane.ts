// useControlPlane.ts - React Query wrapper around the JMCP snapshot.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { ControlPlaneSnapshot } from '../api/types';

export const CONTROL_PLANE_QUERY_KEY: readonly ['control-plane', 'status'] = [
  'control-plane',
  'status',
];

export function useControlPlane(): UseQueryResult<
  ControlPlaneSnapshot,
  Error
> {
  return useQuery({
    queryKey: CONTROL_PLANE_QUERY_KEY,
    queryFn: ({ signal }) =>
      apiGet<ControlPlaneSnapshot>(endpoints.controlPlaneStatus(), { signal }),
    staleTime: 15_000,
    refetchOnWindowFocus: true,
  });
}
