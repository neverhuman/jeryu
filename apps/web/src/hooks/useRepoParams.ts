// useRepoParams.ts — resolve repo route parameters from props or URL.
//
// When the RepoRouter dispatches to a page, it passes provider/fullName
// as props. When a page is rendered directly via a route (e.g. in tests),
// it falls back to useParams().

import { useParams } from 'react-router-dom';
import { parseRepoSplat } from '../pages/RepoRouter';

export interface RepoRouteProps {
  /** Git host / provider (e.g. "jeryu", "github"). */
  provider?: string;
  /** Full "owner/name" repository path. */
  fullName?: string;
}

/**
 * Returns { provider, fullName } from either explicit props or URL params.
 * When called with empty/missing props, falls back to parsing the URL splat.
 */
export function useRepoParams(props?: RepoRouteProps): {
  provider: string;
  fullName: string;
} {
  const params = useParams();

  if (props?.provider && props?.fullName) {
    return { provider: props.provider, fullName: props.fullName };
  }

  // Fallback: parse from URL params (compat with old direct routes or tests).
  const provider = params.provider ?? 'unknown';

  // Try fullNameFromParams pattern (fullName + splat tail).
  const main = params.fullName ?? '';
  const tail = params['*'] ?? '';

  // If there's a splat, parse it to get the full name (stripping sub-paths).
  if (tail) {
    // The splat might contain sub-path segments (agents, code, etc.)
    // We need to reconstruct the full repo name from `fullName/tail`
    // but strip known sub-path prefixes from the tail.
    const { fullName: parsed } = parseRepoSplat(`${main}/${tail}`);
    return { provider, fullName: parsed };
  }
  
  return { provider, fullName: main };
}
