// RepositoryOverviewPage.tsx — Phase 1 stub for W-FE-09.

import { useEffect } from 'react';
import { useParams } from 'react-router-dom';

import { useSelectionStore } from '../stores/selectionStore';
import { useRealtime } from '../hooks/useRealtime';
import { StubPage } from './StubPage';

export function RepositoryOverviewPage(): JSX.Element {
  const params = useParams();
  const provider = params.provider ?? 'unknown';
  const fullName = params.fullName ?? params['*'] ?? '';
  const setRepo = useSelectionStore((s) => s.setCurrentRepo);

  useEffect(() => {
    setRepo(`${provider}:${fullName}`);
    return () => setRepo(null);
  }, [provider, fullName, setRepo]);

  // Once repo_id resolution lands (W-FE-09), swap for `repo.${id}` scopes.
  useRealtime([`repo.${provider}.${fullName}`]);

  return (
    <StubPage
      title={fullName || 'Repository overview'}
      workPackage="W-FE-09"
      description={`Provider: ${provider}. Phase 1 selects the entity into the selection store and subscribes to its activity scope. README rendering, branch selector, and health strip land with W-FE-09.`}
    />
  );
}
