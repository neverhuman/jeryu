// RepositorySettingsPage.tsx — Phase 1 stub for W-FE-12.

import { useParams } from 'react-router-dom';

import { StubPage } from './StubPage';

export function RepositorySettingsPage(): JSX.Element {
  const { section } = useParams();
  return (
    <StubPage
      title={section ? `Repository settings · ${section}` : 'Repository settings'}
      workPackage="W-FE-12"
      description="Searchable left nav with categories. Every change triggers SettingsDiffPreview → confirm → patch."
    />
  );
}
