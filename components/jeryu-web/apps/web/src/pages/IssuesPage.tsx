// IssuesPage.tsx — repo issue-compatible alias for Work Tracker.

import { WorkPage } from './WorkPage';

export interface IssuesPageProps {
  provider?: string;
  fullName?: string;
}

export function IssuesPage(props: IssuesPageProps = {}): JSX.Element {
  return <WorkPage {...props} alias="issues" />;
}
