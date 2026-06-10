// IssuesPage.tsx — issues route; feature content lands with W-FE-13/14.

import { NotImplementedRoute } from './NotImplementedRoute';

export interface IssuesPageProps {
  provider?: string;
  fullName?: string;
}

export function IssuesPage(props: IssuesPageProps = {}): JSX.Element {
  return (
    <NotImplementedRoute
      title="Issues"
      workPackage="W-FE-13/14"
      description="Filters: state, label, assignee, milestone. Inline triage actions land alongside Action UX primitives."
    />
  );
}
