// RepositoryPullRequestsPage.tsx — PR list route; cockpit lands with W-FE-11.

import { NotImplementedRoute } from './NotImplementedRoute';

export function RepositoryPullRequestsPage(): JSX.Element {
  return (
    <NotImplementedRoute
      title="Pull requests"
      workPackage="W-FE-11"
      description="PR list with filters (state, draft, reviewer, blockers). The cockpit lands with W-FE-11."
    />
  );
}
