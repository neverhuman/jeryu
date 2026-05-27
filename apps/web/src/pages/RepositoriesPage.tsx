// RepositoriesPage.tsx — Phase 1 stub for W-FE-08.

import { useSearchParams } from 'react-router-dom';

import { StubPage } from './StubPage';

export interface RepositoriesPageProps {
  /** When `create`, the page renders the "new repo" stub variant. */
  mode?: 'list' | 'create';
}

export function RepositoriesPage({
  mode,
}: RepositoriesPageProps): JSX.Element {
  const [params] = useSearchParams();
  const effectiveMode =
    mode ?? (params.get('new') === '1' ? 'create' : 'list');
  if (effectiveMode === 'create') {
    return (
      <StubPage
        title="Create repository"
        workPackage="W-FE-08"
        description="The 2-step (preview → execute) create dialog lands with W-FE-08."
      />
    );
  }
  return (
    <StubPage
      title="Repositories"
      workPackage="W-FE-08"
      description="Filterable table + card grid with health pills, family grouping, and bulk actions."
    />
  );
}
