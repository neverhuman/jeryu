// SearchResultsPage.tsx — global search results (W-FE-20).
//
// The page reads `?q=...` from the URL so deep links work; typing in the
// input updates the URL via `setSearchParams` and the React Query hook
// rerolls automatically. Debounce window is 50 ms (matches §31 W-FE-20).
//
// Results are grouped by kind. Each section renders an icon, primary text,
// context (full_name / path / commit sha), and navigates on click. Empty,
// loading, and error states reuse the shared state surfaces so the
// page stays consistent with the rest of the SPA.

import {
  AlertCircle,
  FileText,
  Folder,
  GitCommit,
  GitMerge,
  Search,
  User,
} from 'lucide-react';
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';

import { ApiError } from '../api/client';
import {
  EmptyState,
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import {
  searchResultsTotal,
  useSearch,
  type CommitSearchHit,
  type FileSearchHit,
  type IssueSearchHit,
  type PullSearchHit,
  type RepoSearchHit,
  type UserSearchHit,
} from '../hooks/useSearch';

import './page.css';
import './SearchResultsPage.css';

const DEBOUNCE_MS = 50;

interface ResultSectionProps {
  icon: ReactNode;
  title: string;
  count: number;
  children: ReactNode;
}

function ResultSection({
  icon,
  title,
  count,
  children,
}: ResultSectionProps): JSX.Element {
  if (count === 0) return <></>;
  return (
    <section
      className="search-results__section"
      aria-label={`${title} (${count})`}
    >
      <header className="search-results__section-header">
        <span className="search-results__section-icon" aria-hidden="true">
          {icon}
        </span>
        <h2 className="search-results__section-title">{title}</h2>
        <span className="search-results__section-count">{count}</span>
      </header>
      <ul className="search-results__list">{children}</ul>
    </section>
  );
}

function repoHrefFromHit(hit: RepoSearchHit): string {
  return `/repos/${encodeURIComponent(hit.id.host)}/${hit.id.owner}/${hit.id.name}`;
}

function pullHref(hit: PullSearchHit): string {
  return `/repos/jeryu/${hit.repo_id}/pulls/${hit.number}`;
}

function issueHref(hit: IssueSearchHit): string {
  return `/repos/jeryu/${hit.repo_id}/issues#${hit.number}`;
}

function fileHref(hit: FileSearchHit): string {
  return `/repos/jeryu/${hit.repo_id}/blob/${encodeURIComponent(hit.ref_name)}/${hit.path}`;
}

export function SearchResultsPage(): JSX.Element {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const initialQuery = params.get('q') ?? '';
  const [input, setInput] = useState(initialQuery);
  const [debouncedQuery, setDebouncedQuery] = useState(initialQuery);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // `params`/`setParams` get a fresh identity on every render, which would
  // restart the debounce timer below if they were effect dependencies. We
  // read their latest values through a ref (kept current via an effect, not
  // during render) so the debounce effect can depend solely on `input`
  // without an exhaustive-deps suppression.
  const urlSyncRef = useRef({ params, setParams });
  useEffect(() => {
    urlSyncRef.current = { params, setParams };
  }, [params, setParams]);

  // Focus the input when the page mounts so `/` -> /search lands ready
  // to type.
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Keep the URL query string in sync with the debounced search input so
  // deep-links survive a reload + the back/forward stack carries the
  // last query the user was on.
  useEffect(() => {
    const handle = window.setTimeout(() => {
      const { params: current, setParams: commit } = urlSyncRef.current;
      setDebouncedQuery(input);
      const next = new URLSearchParams(current);
      if (input.trim().length === 0) {
        next.delete('q');
      } else {
        next.set('q', input);
      }
      commit(next, { replace: true });
    }, DEBOUNCE_MS);
    return () => window.clearTimeout(handle);
  }, [input]);

  const searchQuery = useSearch(debouncedQuery, { limit: 20 });
  const total = useMemo(
    () => searchResultsTotal(searchQuery.data),
    [searchQuery.data]
  );

  let body: JSX.Element;
  if (debouncedQuery.trim().length === 0) {
    body = (
      <EmptyState
        icon={Search}
        title="Type to search."
        description="Search repositories, files, pull requests, issues, commits, and users. Press / from anywhere to jump here."
      />
    );
  } else if (searchQuery.isPending) {
    body = (
      <LoadingState
        title="Searching…"
        description={`Query: ${debouncedQuery}`}
        variant="message"
      />
    );
  } else if (searchQuery.isError) {
    const err = searchQuery.error;
    if (err instanceof ApiError && err.status === 403) {
      body = (
        <PermissionDeniedState
          description="You do not have permission to use search."
          missingPermission="repo.read"
        />
      );
    } else {
      body = (
        <ErrorState
          title="Search request failed."
          error={searchQuery.error}
        />
      );
    }
  } else if (total === 0) {
    body = (
      <EmptyState
        icon={Search}
        title="No matches."
        description={`Nothing matched "${debouncedQuery}". Try a different term or remove filters.`}
      />
    );
  } else {
    body = (
      <div className="search-results__sections">
        <ResultSection
          icon={<Folder size={14} />}
          title="Repositories"
          count={searchQuery.data?.repos.length ?? 0}
        >
          {(searchQuery.data?.repos ?? []).map((hit) => (
            <li key={hit.id.id} className="search-results__item">
              <Link
                to={repoHrefFromHit(hit)}
                className="search-results__item-link"
              >
                <span className="search-results__primary">
                  {hit.id.owner}/{hit.id.name}
                </span>
                <span className="search-results__context">
                  {hit.description ?? hit.full_name}
                </span>
              </Link>
            </li>
          ))}
        </ResultSection>
        <ResultSection
          icon={<FileText size={14} />}
          title="Files"
          count={searchQuery.data?.files.length ?? 0}
        >
          {(searchQuery.data?.files ?? []).map((hit) => (
            <li
              key={`${hit.repo_id}-${hit.ref_name}-${hit.path}`}
              className="search-results__item"
            >
              <button
                type="button"
                className="search-results__item-link"
                onClick={() => navigate(fileHref(hit))}
              >
                <span className="search-results__primary">{hit.path}</span>
                <span className="search-results__context">
                  {hit.repo_id} · {hit.ref_name}
                </span>
              </button>
            </li>
          ))}
        </ResultSection>
        <ResultSection
          icon={<GitCommit size={14} />}
          title="Commits"
          count={searchQuery.data?.commits.length ?? 0}
        >
          {(searchQuery.data?.commits ?? []).map((hit: CommitSearchHit) => (
            <li
              key={`${hit.repo_id}-${hit.sha}`}
              className="search-results__item"
            >
              <span className="search-results__item-link search-results__item-link--static">
                <span className="search-results__primary">{hit.summary}</span>
                <span className="search-results__context">
                  <code>{hit.sha.slice(0, 7)}</code> · {hit.author} · {hit.repo_id}
                </span>
              </span>
            </li>
          ))}
        </ResultSection>
        <ResultSection
          icon={<GitMerge size={14} />}
          title="Pull requests"
          count={searchQuery.data?.pulls.length ?? 0}
        >
          {(searchQuery.data?.pulls ?? []).map((hit) => (
            <li
              key={`${hit.repo_id}-${hit.number}`}
              className="search-results__item"
            >
              <Link to={pullHref(hit)} className="search-results__item-link">
                <span className="search-results__primary">
                  #{hit.number} {hit.title}
                </span>
                <span className="search-results__context">
                  {hit.repo_id} · {hit.state}
                </span>
              </Link>
            </li>
          ))}
        </ResultSection>
        <ResultSection
          icon={<AlertCircle size={14} />}
          title="Issues"
          count={searchQuery.data?.issues.length ?? 0}
        >
          {(searchQuery.data?.issues ?? []).map((hit) => (
            <li
              key={`${hit.repo_id}-${hit.number}`}
              className="search-results__item"
            >
              <Link to={issueHref(hit)} className="search-results__item-link">
                <span className="search-results__primary">
                  #{hit.number} {hit.title}
                </span>
                <span className="search-results__context">
                  {hit.repo_id} · {hit.state}
                </span>
              </Link>
            </li>
          ))}
        </ResultSection>
        <ResultSection
          icon={<User size={14} />}
          title="Users"
          count={searchQuery.data?.users.length ?? 0}
        >
          {(searchQuery.data?.users ?? []).map((hit: UserSearchHit) => (
            <li key={hit.login} className="search-results__item">
              <span className="search-results__item-link search-results__item-link--static">
                <span className="search-results__primary">@{hit.login}</span>
                {hit.display_name ? (
                  <span className="search-results__context">
                    {hit.display_name}
                  </span>
                ) : null}
              </span>
            </li>
          ))}
        </ResultSection>
      </div>
    );
  }

  return (
    <div className="page search-results">
      <header className="page__header">
        <h1 className="page__title">Search</h1>
        <p className="page__subtitle">
          Search repositories, files, pull requests, issues, commits, and
          users. Backed by <code>/api/v1/search</code>.
        </p>
      </header>
      <div className="search-results__form" role="search">
        <span className="search-results__form-icon" aria-hidden="true">
          <Search size={16} />
        </span>
        <input
          ref={inputRef}
          type="search"
          className="search-results__input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          aria-label="Search query"
          autoFocus
        />
        {total > 0 ? (
          <span
            className="search-results__total"
            aria-label={`${total} matches`}
          >
            {total} {total === 1 ? 'match' : 'matches'}
          </span>
        ) : null}
      </div>
      {body}
    </div>
  );
}
