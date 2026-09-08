// RepositoriesPage.tsx — Phase 2 implementation (W-FE-08).
//
// Renders the repository list/grid with a debounced search input, filter
// chips, sort dropdown, and a card/table view toggle whose state is
// persisted in `preferencesStore.reposView`. The "Create repo" button
// surfaces the 2-step preview→execute dialog (`CreateRepoDialog`).
//
// All five UX-QA states are wired:
//   * loading       — initial fetch / pending
//   * empty         — `total === 0`
//   * error         — non-403 ApiError
//   * permission    — 403 from the list endpoint
//   * success       — family tiles first, then single repo cards.

import { LayoutGrid, Plus, Table } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import {
  useLocation,
  useNavigate,
  useSearchParams,
} from 'react-router-dom';

import { ActionButton } from '../components/action/ActionButton';
import { CreateRepoDialog } from '../components/repo';
import {
  useRepositories,
  type RepoSort,
  type RepositoriesQuery,
} from '../hooks/useRepositories';
import { usePreferencesStore } from '../stores/preferencesStore';
import {
  DEFAULT_FILTER,
  FilterChips,
  RepositoriesBody,
  type FilterState,
} from './repositoriesPageParts';

import '../components/repo/repo.css';
import './page.css';

export interface RepositoriesPageProps {
  /** When `create`, the page opens the create dialog on mount. */
  mode?: 'list' | 'create';
}

export function RepositoriesPage({
  mode,
}: RepositoriesPageProps): JSX.Element {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const location = useLocation();
  const effectiveMode =
    mode ?? (params.get('new') === '1' ? 'create' : 'list');

  const reposView = usePreferencesStore((s) => s.reposView);
  const setReposView = usePreferencesStore((s) => s.setReposView);

  const [filter, setFilter] = useState<FilterState>(DEFAULT_FILTER);
  const [searchInput, setSearchInput] = useState('');
  const [dialogOpen, setDialogOpen] = useState(effectiveMode === 'create');

  // Debounce search input → filter.search (100 ms per the plan).
  useEffect(() => {
    const handle = setTimeout(() => {
      setFilter((prev) =>
        prev.search === searchInput ? prev : { ...prev, search: searchInput }
      );
    }, 100);
    return () => clearTimeout(handle);
  }, [searchInput]);

  // Sync dialog open state with the route. If the user navigates with the
  // `?new=1` param after mount, surface the dialog.
  useEffect(() => {
    if (params.get('new') === '1') {
      setDialogOpen(true);
    }
  }, [params]);

  const query: RepositoriesQuery = useMemo(
    () => ({
      search: filter.search || undefined,
      host: filter.host,
      visibility: filter.visibility,
      family: filter.family,
      archived: filter.archived || undefined,
      sort: filter.sort,
    }),
    [filter]
  );
  const list = useRepositories(query);

  const closeDialog = (): void => {
    setDialogOpen(false);
    if (location.pathname === '/repos/new' || params.get('new') === '1') {
      navigate('/repos', { replace: true });
    }
  };

  const repos = list.data?.repositories ?? [];
  const facets = list.data?.facets;

  return (
    <div className="page page--wide" data-testid="repositories-page">
      <header className="page__header">
        <div className="page__welcome">
          <h1 className="page__title">Repositories</h1>
        </div>
        <p className="page__subtitle">
          Browse, search, and create repositories across all hosts.
        </p>
        <div className="repo-toolbar">
          <div className="repo-toolbar__search">
            <label htmlFor="repos-search" className="sr-only">
              Search repositories
            </label>
            <input
              id="repos-search"
              type="search"
              className="repo-toolbar__input"
              value={searchInput}
              onChange={(e) => setSearchInput(e.target.value)}
              aria-label="Search repositories"
            />
          </div>

          <FilterChips
            label="Host"
            value={filter.host}
            options={
              facets?.hosts ?? ['jeryu', 'local']
            }
            onChange={(host) =>
              setFilter((prev) => ({ ...prev, host }))
            }
            ariaLabel="Filter by host"
          />

          <FilterChips
            label="Visibility"
            value={filter.visibility}
            options={['public', 'internal', 'private']}
            onChange={(v) =>
              setFilter((prev) => ({
                ...prev,
                visibility: v as 'public' | 'internal' | 'private' | undefined,
              }))
            }
            ariaLabel="Filter by visibility"
          />

          {facets && facets.families.length > 0 ? (
            <FilterChips
              label="Family"
              value={filter.family}
              options={facets.families}
              onChange={(family) =>
                setFilter((prev) => ({ ...prev, family }))
              }
              ariaLabel="Filter by family"
            />
          ) : null}

          <label className="repo-toolbar__chip">
            <input
              type="checkbox"
              checked={filter.archived}
              onChange={(e) =>
                setFilter((prev) => ({
                  ...prev,
                  archived: e.target.checked,
                }))
              }
            />
            Archived
          </label>

          <label className="sr-only" htmlFor="repos-sort">
            Sort repositories
          </label>
          <select
            id="repos-sort"
            className="repo-toolbar__select"
            value={filter.sort}
            onChange={(e) =>
              setFilter((prev) => ({
                ...prev,
                sort: e.target.value as RepoSort,
              }))
            }
          >
            <option value="recent_activity">Recent activity</option>
            <option value="name">Name</option>
            <option value="open_prs">Open PRs</option>
            <option value="failing_checks">Failing checks</option>
          </select>

          <div className="repo-toolbar__group" role="radiogroup" aria-label="View">
            <ActionButton
              variant={reposView === 'card' ? 'primary' : 'ghost'}
              onClick={() => setReposView('card')}
              role="radio"
              aria-checked={reposView === 'card'}
              icon={<LayoutGrid size={12} aria-hidden="true" />}
              aria-label="Card view"
            >
              Cards
            </ActionButton>
            <ActionButton
              variant={reposView === 'table' ? 'primary' : 'ghost'}
              onClick={() => setReposView('table')}
              role="radio"
              aria-checked={reposView === 'table'}
              icon={<Table size={12} aria-hidden="true" />}
              aria-label="Table view"
            >
              Table
            </ActionButton>
          </div>

          <ActionButton
            variant="primary"
            icon={<Plus size={14} aria-hidden="true" />}
            onClick={() => setDialogOpen(true)}
            aria-label="Create repository"
          >
            Create repo
          </ActionButton>
        </div>
      </header>

      <RepositoriesBody
        loading={list.isPending}
        error={list.error}
        repos={repos}
        view={reposView}
        search={filter.search}
        familyFilter={filter.family}
        onClearFilters={() => {
          setFilter(DEFAULT_FILTER);
          setSearchInput('');
        }}
      />

      <CreateRepoDialog
        open={dialogOpen}
        onCancel={closeDialog}
        onCreated={() => {
          list.refetch();
        }}
      />
    </div>
  );
}
