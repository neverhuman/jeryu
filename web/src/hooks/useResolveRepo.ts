// useResolveRepo.ts — resolve URL params to a backend `RepositoryId` (W-FE-09).
//
// Frontend routes use the human-readable `:provider/*fullName` (where
// `fullName` is the owner + name path, possibly nested for namespaces).
// The backend keys repositories by an opaque UUID (`RepositoryId.id`).
//
// This hook first checks the repositories list cache, then falls back to
// performing a fresh `GET /api/v1/repos` query so deep links work on first
// load. The shape is intentionally compatible with `useQuery` so consumers
// can treat it uniformly.

import { useMemo } from 'react';

import type { RepositorySummary } from '../api/types';
import { useRepositories } from './useRepositories';

export interface ResolvedRepo {
  /** Opaque backend id used in /api/v1/repos/{id}. */
  id: string;
  /** Full `RepositorySummary` from the list cache. */
  summary: RepositorySummary;
}

export interface UseResolveRepoResult {
  isPending: boolean;
  isError: boolean;
  error: Error | null;
  data: ResolvedRepo | null;
}

function matches(
  summary: RepositorySummary,
  host: string,
  fullName: string
): boolean {
  if (summary.id.host !== host) return false;
  // `fullName` is what comes after `:provider/`. For a normal repo it equals
  // `<owner>/<name>`. We accept either:
  //   * `${owner}/${name}` directly, or
  //   * the longer nested namespace path where `owner` already contains
  //     intermediate namespace segments.
  const direct = `${summary.id.owner}/${summary.id.name}`;
  return fullName === direct;
}

export function useResolveRepo(
  provider: string,
  fullName: string
): UseResolveRepoResult {
  // Use a minimal query — the host filter narrows the list, the rest is
  // matched in memory. This is the agreed §35.1.2 contract: the backend
  // exposes list-with-host-filter, and the SPA resolves the full name
  // client-side against that page.
  const list = useRepositories({ host: provider });

  const data = useMemo<ResolvedRepo | null>(() => {
    if (!list.data) return null;
    const summary = list.data.repositories.find((r) =>
      matches(r, provider, fullName)
    );
    if (!summary) return null;
    return { id: summary.id.id, summary };
  }, [list.data, provider, fullName]);

  return {
    isPending: list.isPending,
    isError: list.isError,
    error: list.error ?? null,
    data,
  };
}
