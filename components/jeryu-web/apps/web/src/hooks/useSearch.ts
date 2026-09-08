// useSearch.ts — React Query wrapper around `GET /api/v1/search` (W-FE-20).
//
// The backend (jeryu-api `rest/search.rs` over `jeryu-core` search) returns
// a shape-stable `SearchResults` with per-kind vectors. We mirror the wire
// types here rather than re-exporting from `contracts/generated/` because
// the backend has not yet emitted ts-rs derives for the search module —
// when it does, the shape moves to `api/types.ts` like the other DTOs.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';

export type SearchKindString =
  | 'repo'
  | 'file'
  | 'commit'
  | 'pull'
  | 'issue'
  | 'user';

export interface RepoSearchHit {
  id: { id: string; host: string; owner: string; name: string };
  full_name: string;
  description?: string;
  updated_at: string;
}

export interface FileSearchHit {
  repo_id: string;
  path: string;
  ref_name: string;
  score: number;
}

export interface CommitSearchHit {
  repo_id: string;
  sha: string;
  summary: string;
  author: string;
}

export interface PullSearchHit {
  repo_id: string;
  number: number;
  title: string;
  state: string;
}

export interface IssueSearchHit {
  repo_id: string;
  number: number;
  title: string;
  state: string;
}

export interface UserSearchHit {
  login: string;
  display_name?: string;
}

export interface SearchResults {
  query: string;
  repos: RepoSearchHit[];
  files: FileSearchHit[];
  commits: CommitSearchHit[];
  pulls: PullSearchHit[];
  issues: IssueSearchHit[];
  users: UserSearchHit[];
  total: number;
}

export interface UseSearchOptions {
  /** Optional set of kinds to scope the query. Defaults to "all kinds". */
  kinds?: ReadonlyArray<SearchKindString>;
  /** Override the per-kind hit limit. Backend clamps to 100. */
  limit?: number;
  /** Override the default 30 s stale time. */
  staleTime?: number;
}

export function useSearch(
  q: string,
  options: UseSearchOptions = {}
): UseQueryResult<SearchResults, Error> {
  const kinds = options.kinds ?? [];
  const trimmed = q.trim();
  return useQuery<SearchResults, Error>({
    queryKey: ['search', trimmed, kinds.join(','), options.limit ?? 20],
    queryFn: ({ signal }) =>
      apiGet<SearchResults>(
        endpoints.search(trimmed, {
          kinds: kinds.length > 0 ? kinds : undefined,
          limit: options.limit,
        }),
        { signal }
      ),
    enabled: trimmed.length > 0,
    staleTime: options.staleTime ?? 30_000,
  });
}

/** Total result count across every kind in a `SearchResults`. */
export function searchResultsTotal(results: SearchResults | undefined): number {
  if (!results) return 0;
  return (
    results.repos.length +
    results.files.length +
    results.commits.length +
    results.pulls.length +
    results.issues.length +
    results.users.length
  );
}
