// useToolingEvidence.ts - read-only ecosystem and tool-build evidence hooks.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type {
  EcosystemResponse,
  ToolBuildClustersResponse,
} from '../api/types';

export function useEcosystem(): UseQueryResult<EcosystemResponse, Error> {
  return useQuery({
    queryKey: ['ecosystem'],
    queryFn: ({ signal }) =>
      apiGet<EcosystemResponse>(endpoints.ecosystem(), { signal }),
    staleTime: 30_000,
  });
}

export function useToolBuildClusters(
  limit = 8
): UseQueryResult<ToolBuildClustersResponse, Error> {
  return useQuery({
    queryKey: ['tool-build-clusters', limit],
    queryFn: ({ signal }) =>
      apiGet<ToolBuildClustersResponse>(
        endpoints.toolBuildClusters({ limit }),
        { signal }
      ),
    staleTime: 30_000,
  });
}
