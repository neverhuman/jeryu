// RepositoryCodePage.tsx — Phase 1 stub for W-FE-10.

import { StubPage } from './StubPage';

export function RepositoryCodePage(): JSX.Element {
  return (
    <StubPage
      title="Code browser"
      workPackage="W-FE-10"
      description="Virtualized file tree + Monaco file viewer. Monaco is React.lazy() loaded so the initial bundle stays small."
    />
  );
}
